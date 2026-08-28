//! Inbound calls from a payment processor — Stripe, today.
//!
//! Deliberately outside every other route's shape: no `TenantScope` (Stripe
//! has no bearer token and knows nothing about our tenancy — the gym, invoice
//! and member all come back out of the event's own metadata, set when the
//! checkout session was created), and no `Json` extractor (signature
//! verification needs the exact bytes Stripe sent, not a round-trip through
//! serde — see `verify_webhook_signature`'s doc).

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use gym_domain::{GymId, InvoiceId, UserId};
use gym_infrastructure::verify_webhook_signature;
use uuid::Uuid;

use crate::state::AppState;

/// Stripe's tolerance for how stale a signed timestamp may be before it is
/// refused as a possible replay — matches Stripe's own client libraries.
const SIGNATURE_TOLERANCE_SECS: i64 = 300;

#[utoipa::path(
    post,
    path = "/api/v1/webhooks/stripe",
    tag = "billing",
    // `String` here documents the *shape* for OpenAPI's benefit — the handler
    // itself reads `axum::body::Bytes`, which has no `ToSchema` impl (it isn't
    // meant to be one; verifying a signature needs the exact bytes, not a
    // type the schema generator can introspect). This annotation decouples
    // the two rather than making the extractor type double as documentation.
    request_body(
        content = String,
        content_type = "application/json",
        description = "Raw Stripe event payload — trust comes from the Stripe-Signature header, not this schema",
    ),
    responses(
        (status = 200, description = "Handled (or a type this deployment ignores)"),
        (status = 400, description = "Missing/invalid signature, or an unreadable event"),
        (status = 503, description = "Stripe is not configured for this deployment"),
    )
)]
pub async fn stripe_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(secret) = state.stripe_webhook_secret.as_deref() else {
        tracing::warn!("stripe webhook received but STRIPE_WEBHOOK_SECRET is not set");
        return StatusCode::SERVICE_UNAVAILABLE;
    };

    let Some(signature) = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
    else {
        return StatusCode::BAD_REQUEST;
    };

    if !verify_webhook_signature(
        &body,
        signature,
        secret,
        Utc::now().timestamp(),
        SIGNATURE_TOLERANCE_SECS,
    ) {
        tracing::warn!("stripe webhook signature verification failed");
        return StatusCode::BAD_REQUEST;
    }

    let Ok(event) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return StatusCode::BAD_REQUEST;
    };

    // Stripe sends many event types; a checkout session is the only one this
    // deployment acts on. Acknowledging (200) rather than erroring on the
    // rest is what stops Stripe from retrying events forever.
    if event.get("type").and_then(|v| v.as_str()) != Some("checkout.session.completed") {
        return StatusCode::OK;
    }

    let parsed = event.pointer("/data/object").and_then(|object| {
        let session_id = object.get("id")?.as_str()?.to_owned();
        let amount_total = object.get("amount_total")?.as_i64()?;
        let metadata = object.get("metadata")?;
        let gym_id: Uuid = metadata.get("gym_id")?.as_str()?.parse().ok()?;
        let invoice_id: Uuid = metadata.get("invoice_id")?.as_str()?.parse().ok()?;
        let member_id: Uuid = metadata.get("member_id")?.as_str()?.parse().ok()?;
        Some((session_id, amount_total, gym_id, invoice_id, member_id))
    });

    let Some((session_id, amount_total, gym_id, invoice_id, member_id)) = parsed else {
        // Our own checkout sessions always carry this metadata (see
        // `create_checkout_session`); one that doesn't was not created by us.
        tracing::error!("checkout.session.completed carried no usable metadata");
        return StatusCode::BAD_REQUEST;
    };

    let result = state
        .billing
        .apply_gateway_payment(gym_application::billing::GatewayPaymentCommand {
            gym_id: GymId::from(gym_id),
            member_id: UserId::from(member_id),
            invoice_id: InvoiceId::from(invoice_id),
            amount_minor: amount_total,
            provider: gym_domain::billing::PaymentProvider::Stripe,
            session_id,
            received_on: Utc::now().date_naive(),
        })
        .await;

    match result {
        Ok(()) => StatusCode::OK,
        Err(error) => {
            tracing::error!(?error, "failed to apply a confirmed stripe payment");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
