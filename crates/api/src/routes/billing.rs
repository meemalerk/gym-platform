//! Billing endpoints — the gym→member side (ADR-0010).
//!
//! Money is managed by managers: every write here is `can_manage_gym`. Reads
//! narrow to the caller — a member sees their own subscription and invoices and
//! nobody else's, because an invoice names a person, an amount and a plan.
//!
//! `is_overdue` is computed per response from today's date, never stored: a
//! status that depends on the calendar cannot sit in a column without a nightly
//! job to keep it honest.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{NaiveDate, Utc};
use gym_application::billing::{
    CreatePlanCommand, IssueInvoiceCommand, RecordPaymentCommand, SubscribeCommand,
};
use gym_domain::{
    InvoiceId, MembershipPlanId, SubscriptionId, UserId,
    billing::{
        BillingInterval, Invoice, InvoiceStatus, MemberSubscription, MembershipPlan, Payment,
        PaymentProvider, SubscriptionStatus, format_money,
    },
    entitlement::Feature,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

// ------------------------------------------------------------------ plans

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreatePlanRequest {
    pub name: String,
    #[schema(nullable)]
    pub description: Option<String>,
    /// Minor units — pence, cents. Never a decimal.
    pub price_minor: i64,
    /// Three-letter code, e.g. "GBP".
    pub currency: String,
    pub interval: BillingInterval,
    /// What buying this plan confers. Required rather than defaulted: a plan
    /// created without stating its grants would silently become a gym-access
    /// plan, which is exactly the mistake that makes a "Coaching" plan not
    /// coach anybody.
    pub grants: Vec<Feature>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PlanResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub price_minor: i64,
    pub currency: String,
    /// Pre-formatted so an invoice, a receipt and a dashboard cannot disagree.
    pub price_label: String,
    pub interval: BillingInterval,
    /// What this plan confers — the same vocabulary the entitlement endpoint
    /// answers in, so a "what do I get" screen needs no mapping table.
    pub grants: Vec<Feature>,
    pub is_offered: bool,
    pub active_subscribers: i64,
}

fn plan_response(plan: MembershipPlan, active_subscribers: i64) -> PlanResponse {
    PlanResponse {
        id: plan.id.into_uuid(),
        price_label: format_money(plan.price_minor, &plan.currency),
        name: plan.name,
        description: plan.description,
        price_minor: plan.price_minor,
        currency: plan.currency,
        interval: plan.interval,
        grants: plan.grants,
        is_offered: plan.archived_at.is_none(),
        active_subscribers,
    }
}

/// What this gym sells. Readable by anyone in the gym — a member choosing what
/// to join has to be able to see the prices.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/plans",
    tag = "billing",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses((status = 200, description = "Plans", body = Vec<PlanResponse>))
)]
pub async fn list_plans(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<PlanResponse>>, ApiError> {
    let views = state.billing.list_plans(&tenant).await?;
    Ok(Json(
        views
            .into_iter()
            .map(|v| plan_response(v.plan, v.active_subscribers))
            .collect(),
    ))
}

/// Add a plan to what this gym offers.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/plans",
    tag = "billing",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = CreatePlanRequest,
    responses(
        (status = 201, description = "Created", body = PlanResponse),
        (status = 403, description = "Managers only"),
    )
)]
pub async fn create_plan(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<CreatePlanRequest>,
) -> Result<(StatusCode, Json<PlanResponse>), ApiError> {
    let plan = state
        .billing
        .create_plan(
            &tenant,
            CreatePlanCommand {
                name: body.name,
                description: body.description,
                price_minor: body.price_minor,
                currency: body.currency,
                interval: body.interval,
                grants: body.grants,
            },
        )
        .await?;

    Ok((StatusCode::CREATED, Json(plan_response(plan, 0))))
}

/// Stop offering a plan. Everyone already on it keeps their terms.
#[utoipa::path(
    delete,
    path = "/api/v1/gyms/{gym_id}/plans/{plan_id}",
    tag = "billing",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("plan_id" = Uuid, Path, description = "Plan id"),
    ),
    responses(
        (status = 204, description = "Archived"),
        (status = 409, description = "Already archived"),
    )
)]
pub async fn archive_plan(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, plan_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .billing
        .archive_plan(&tenant, MembershipPlanId::from(plan_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------- subscriptions

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SubscribeRequest {
    pub member_id: Uuid,
    pub plan_id: Uuid,
    pub started_on: NaiveDate,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SubscriptionResponse {
    pub id: Uuid,
    pub member_id: Uuid,
    pub member_name: String,
    pub plan_id: Uuid,
    pub plan_name: String,
    pub price_minor: i64,
    pub currency: String,
    pub price_label: String,
    pub status: SubscriptionStatus,
    pub is_active: bool,
    pub started_on: NaiveDate,
    #[schema(nullable)]
    pub next_charge_on: Option<NaiveDate>,
}

fn subscription_response(
    s: MemberSubscription,
    member_name: String,
    plan_name: String,
) -> SubscriptionResponse {
    SubscriptionResponse {
        id: s.id.into_uuid(),
        member_id: s.member_id.into_uuid(),
        member_name,
        plan_id: s.plan_id.into_uuid(),
        plan_name,
        price_label: format_money(s.price_minor, &s.currency),
        price_minor: s.price_minor,
        currency: s.currency,
        is_active: s.status.is_active(),
        status: s.status,
        started_on: s.started_on,
        next_charge_on: s.next_charge_on,
    }
}

/// Subscriptions: the gym's for a manager, your own otherwise.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/subscriptions",
    tag = "billing",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses((status = 200, description = "Subscriptions", body = Vec<SubscriptionResponse>))
)]
pub async fn list_subscriptions(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<SubscriptionResponse>>, ApiError> {
    let views = state.billing.list_subscriptions(&tenant).await?;
    Ok(Json(
        views
            .into_iter()
            .map(|v| subscription_response(v.subscription, v.member_name, v.plan_name))
            .collect(),
    ))
}

/// What subscribing produced: the standing arrangement, and the first bill.
///
/// Both, deliberately. This used to return only the invoice, which meant a
/// caller could not reference the subscription it had just created — and a
/// verification suite that captured `id` from the response and used it as a
/// subscription id matched nothing, silently, forever. Two things were created;
/// the response says so.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SubscribeResponse {
    pub subscription: SubscriptionResponse,
    /// The first period, billed up front — a gym takes the first month at
    /// signup. Subsequent periods come from the nightly billing tick.
    pub invoice: InvoiceResponse,
}

/// Put somebody on a plan. Issues their first invoice in the same request.
///
/// A manager may subscribe anybody. A **member may subscribe themselves** to a
/// plan the gym currently offers (ADR-0031) — that is the way out of "you
/// cannot start a workout until you are on a plan", which was otherwise a dead
/// end inside the app.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/subscriptions",
    tag = "billing",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = SubscribeRequest,
    responses(
        (status = 201, description = "Subscribed", body = SubscribeResponse),
        (status = 403, description = "Subscribing somebody else, without managing the gym"),
        (status = 404, description = "No such plan, not on sale, or not a member here"),
        (status = 409, description = "Already on this plan, or already on one"),
    )
)]
pub async fn subscribe(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<SubscribeRequest>,
) -> Result<(StatusCode, Json<SubscribeResponse>), ApiError> {
    let (subscription, invoice) = state
        .billing
        .subscribe(
            &tenant,
            SubscribeCommand {
                member_id: UserId::from(body.member_id),
                plan_id: MembershipPlanId::from(body.plan_id),
                started_on: body.started_on,
            },
        )
        .await?;

    let today = Utc::now().date_naive();
    // Names are not to hand here and are not worth a second query: the caller
    // just named the member and the plan. The list read carries them.
    Ok((
        StatusCode::CREATED,
        Json(SubscribeResponse {
            subscription: subscription_response(subscription, String::new(), String::new()),
            invoice: invoice_response(invoice, String::new(), 0, today),
        }),
    ))
}

/// Stop a subscription. Access runs to the end of the paid period.
#[utoipa::path(
    delete,
    path = "/api/v1/gyms/{gym_id}/subscriptions/{subscription_id}",
    tag = "billing",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("subscription_id" = Uuid, Path, description = "Subscription id"),
    ),
    responses(
        (status = 200, description = "Cancelled; access runs to `ends_on`", body = SubscriptionResponse),
        (status = 403, description = "Managers only"),
        (status = 404, description = "No such subscription"),
        (status = 409, description = "Already cancelled"),
    )
)]
pub async fn cancel_subscription(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, subscription_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<SubscriptionResponse>, ApiError> {
    let subscription = state
        .billing
        .cancel_subscription(&tenant, SubscriptionId::from(subscription_id))
        .await?;

    Ok(Json(subscription_response(
        subscription,
        String::new(),
        String::new(),
    )))
}

// --------------------------------------------------------------- invoices

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct IssueInvoiceRequest {
    pub member_id: Uuid,
    #[schema(nullable)]
    pub subscription_id: Option<Uuid>,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    #[schema(nullable)]
    pub period_start: Option<NaiveDate>,
    #[schema(nullable)]
    pub period_end: Option<NaiveDate>,
    pub due_on: NaiveDate,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct VoidInvoiceRequest {
    #[schema(nullable)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct InvoiceResponse {
    pub id: Uuid,
    pub member_id: Uuid,
    pub member_name: String,
    /// Which subscription this bills, if any. Null for an ad-hoc charge.
    ///
    /// Absent until now, which meant a member holding two memberships could
    /// not tell which invoice belonged to which — and nothing could group a
    /// billing history by the arrangement that produced it.
    #[schema(nullable)]
    pub subscription_id: Option<Uuid>,
    pub reference: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub amount_label: String,
    #[schema(nullable)]
    pub period_start: Option<NaiveDate>,
    #[schema(nullable)]
    pub period_end: Option<NaiveDate>,
    pub issued_on: NaiveDate,
    pub due_on: NaiveDate,
    pub status: InvoiceStatus,
    /// Computed against today, never stored — see the module note.
    pub is_overdue: bool,
    pub days_overdue: i64,
    /// What has been received against it, refunds included.
    pub paid_minor: i64,
}

fn invoice_response(
    invoice: Invoice,
    member_name: String,
    paid_minor: i64,
    today: NaiveDate,
) -> InvoiceResponse {
    InvoiceResponse {
        id: invoice.id.into_uuid(),
        member_id: invoice.member_id.into_uuid(),
        member_name,
        subscription_id: invoice.subscription_id.map(SubscriptionId::into_uuid),
        amount_label: format_money(invoice.amount_minor, &invoice.currency),
        is_overdue: invoice.is_overdue(today),
        days_overdue: invoice.days_overdue(today),
        reference: invoice.reference,
        description: invoice.description,
        amount_minor: invoice.amount_minor,
        currency: invoice.currency,
        period_start: invoice.period.map(|(s, _)| s),
        period_end: invoice.period.map(|(_, e)| e),
        issued_on: invoice.issued_on,
        due_on: invoice.due_on,
        status: invoice.status,
        paid_minor,
    }
}

/// Invoices: the gym's for a manager, your own otherwise.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/invoices",
    tag = "billing",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses((status = 200, description = "Invoices", body = Vec<InvoiceResponse>))
)]
pub async fn list_invoices(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<InvoiceResponse>>, ApiError> {
    let today = Utc::now().date_naive();
    let views = state.billing.list_invoices(&tenant).await?;
    Ok(Json(
        views
            .into_iter()
            .map(|v| invoice_response(v.invoice, v.member_name, v.paid_minor, today))
            .collect(),
    ))
}

/// Issue an invoice by hand — a drop-in, a kit sale, a correction.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/invoices",
    tag = "billing",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = IssueInvoiceRequest,
    responses(
        (status = 201, description = "Issued", body = InvoiceResponse),
        (status = 403, description = "Managers only"),
    )
)]
pub async fn issue_invoice(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<IssueInvoiceRequest>,
) -> Result<(StatusCode, Json<InvoiceResponse>), ApiError> {
    let period = match (body.period_start, body.period_end) {
        (Some(s), Some(e)) => Some((s, e)),
        _ => None,
    };

    let invoice = state
        .billing
        .issue_invoice(
            &tenant,
            IssueInvoiceCommand {
                member_id: UserId::from(body.member_id),
                subscription_id: body.subscription_id.map(SubscriptionId::from),
                description: body.description,
                amount_minor: body.amount_minor,
                currency: body.currency,
                period,
                due_on: body.due_on,
            },
        )
        .await?;

    let today = Utc::now().date_naive();
    Ok((
        StatusCode::CREATED,
        Json(invoice_response(invoice, String::new(), 0, today)),
    ))
}

/// Void an invoice issued in error. A paid one is refunded, never voided.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/invoices/{invoice_id}/void",
    tag = "billing",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("invoice_id" = Uuid, Path, description = "Invoice id"),
    ),
    request_body = VoidInvoiceRequest,
    responses(
        (status = 200, description = "Voided", body = InvoiceResponse),
        (status = 403, description = "Managers only"),
        (status = 409, description = "Already settled"),
    )
)]
pub async fn void_invoice(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, invoice_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<VoidInvoiceRequest>,
) -> Result<Json<InvoiceResponse>, ApiError> {
    let invoice = state
        .billing
        .void_invoice(&tenant, InvoiceId::from(invoice_id), body.reason)
        .await?;

    let today = Utc::now().date_naive();
    Ok(Json(invoice_response(invoice, String::new(), 0, today)))
}

// --------------------------------------------------------------- payments

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RecordPaymentRequest {
    /// Minor units. Negative is a refund.
    pub amount_minor: i64,
    pub provider: PaymentProvider,
    #[schema(nullable)]
    pub provider_ref: Option<String>,
    pub received_on: NaiveDate,
    #[schema(nullable)]
    pub note: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PaymentResponse {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub amount_minor: i64,
    pub currency: String,
    pub amount_label: String,
    pub provider: PaymentProvider,
    #[schema(nullable)]
    pub provider_ref: Option<String>,
    pub received_on: NaiveDate,
    pub recorded_by: Uuid,
    #[schema(nullable)]
    pub note: Option<String>,
}

impl From<Payment> for PaymentResponse {
    fn from(p: Payment) -> Self {
        Self {
            id: p.id.into_uuid(),
            invoice_id: p.invoice_id.into_uuid(),
            amount_label: format_money(p.amount_minor, &p.currency),
            amount_minor: p.amount_minor,
            currency: p.currency,
            provider: p.provider,
            provider_ref: p.provider_ref,
            received_on: p.received_on,
            recorded_by: p.recorded_by.into_uuid(),
            note: p.note,
        }
    }
}

/// Record money received against an invoice — cash at the desk, a terminal, or
/// later a provider. Settles the invoice when it covers the balance.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/invoices/{invoice_id}/payments",
    tag = "billing",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("invoice_id" = Uuid, Path, description = "Invoice id"),
    ),
    request_body = RecordPaymentRequest,
    responses(
        (status = 201, description = "Recorded", body = PaymentResponse),
        (status = 403, description = "Managers only"),
        (status = 404, description = "No such invoice"),
    )
)]
pub async fn record_payment(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, invoice_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<RecordPaymentRequest>,
) -> Result<(StatusCode, Json<PaymentResponse>), ApiError> {
    let payment = state
        .billing
        .record_payment(
            &tenant,
            RecordPaymentCommand {
                invoice_id: InvoiceId::from(invoice_id),
                amount_minor: body.amount_minor,
                provider: body.provider,
                provider_ref: body.provider_ref,
                received_on: body.received_on,
                note: body.note,
            },
        )
        .await?;

    Ok((StatusCode::CREATED, Json(payment.into())))
}

/// Everything received against one invoice — the payment timeline.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/invoices/{invoice_id}/payments",
    tag = "billing",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("invoice_id" = Uuid, Path, description = "Invoice id"),
    ),
    responses((status = 200, description = "Payments", body = Vec<PaymentResponse>))
)]
pub async fn list_payments(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, invoice_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<PaymentResponse>>, ApiError> {
    // The service scopes this: a member reads the payments on their own
    // invoice — it is their receipt — and anyone else gets "not found".
    let payments = state
        .billing
        .payments_for(&tenant, InvoiceId::from(invoice_id))
        .await?;
    Ok(Json(payments.into_iter().map(Into::into).collect()))
}

// ------------------------------------------------------------- self-pay

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateCheckoutSessionRequest {
    /// Where to send the payer back to afterwards — the app supplies this
    /// (`Linking.createURL(...)`) because it, not the server, knows whether it
    /// is running native (a custom scheme) or on the web (an http origin).
    /// The server appends `?status=success` / `?status=cancel`.
    pub return_url: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CheckoutSessionResponse {
    /// The gateway's reference for this attempt. Not needed to complete the
    /// payment — the hosted page carries it — but returned so a client can
    /// correlate a redirect it started with the payment that later appears.
    pub provider_ref: String,
    /// Hand this straight to a browser — Stripe's own hosted payment page.
    pub checkout_url: String,
}

/// Start a Stripe Checkout for this invoice's outstanding balance — the
/// self-service half of billing. The invoice owner, or a manager on their
/// behalf, may call this; nothing about the invoice changes until Stripe
/// confirms payment via webhook.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/invoices/{invoice_id}/checkout",
    tag = "billing",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("invoice_id" = Uuid, Path, description = "Invoice id"),
    ),
    request_body = CreateCheckoutSessionRequest,
    responses(
        (status = 201, description = "Checkout session created", body = CheckoutSessionResponse),
        (status = 404, description = "No such invoice, or not yours"),
        (status = 409, description = "Not open for payment"),
        (status = 503, description = "Card payment is not configured for this gym"),
    )
)]
pub async fn create_checkout_session(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, invoice_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CreateCheckoutSessionRequest>,
) -> Result<(StatusCode, Json<CheckoutSessionResponse>), ApiError> {
    let session = state
        .billing
        .create_checkout_session(
            &tenant,
            InvoiceId::from(invoice_id),
            format!("{}?status=success", body.return_url),
            format!("{}?status=cancel", body.return_url),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CheckoutSessionResponse {
            checkout_url: session.url,
            provider_ref: session.provider_ref,
        }),
    ))
}
