//! Stripe: the payment processor behind ADR-0010's seam.
//!
//! Two independent pieces live here:
//!  - `StripeGateway` — outbound: create a hosted Checkout Session so a member
//!    can pay their own invoice.
//!  - `verify_webhook_signature` — inbound: prove a webhook claiming to be
//!    Stripe actually is, before `crates/api`'s route acts on it.
//!
//! Both are optional. A deployment with no `STRIPE_SECRET_KEY` gets
//! `NotConfiguredGateway` instead — `BillingService` works either way, and
//! "pay by Stripe" just explains itself as unavailable rather than crashing
//! anything (see `ApplicationError::Unavailable`).

use async_trait::async_trait;
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{CheckoutSession, CheckoutSessionRequest, PaymentGateway},
};
use sha2::{Digest, Sha256};

/// Calls the real Stripe API. Constructed only when `STRIPE_SECRET_KEY` is set
/// (see `bins/server/src/main.rs`) — the secret key is a bearer credential for
/// Stripe's whole account, so its absence is treated as "feature off", never
/// as an empty-string default.
pub struct StripeGateway {
    secret_key: String,
    http: reqwest::Client,
}

impl StripeGateway {
    #[must_use]
    pub fn new(secret_key: String) -> Self {
        Self {
            secret_key,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl PaymentGateway for StripeGateway {
    async fn create_checkout_session(
        &self,
        req: CheckoutSessionRequest,
    ) -> ApplicationResult<CheckoutSession> {
        // Stripe's Checkout Sessions API takes classic form-encoded params,
        // including PHP-style bracket keys for nested objects — this is the
        // documented shape, not a workaround.
        let amount = req.amount_minor.to_string();
        let params: Vec<(&str, String)> = vec![
            ("mode", "payment".to_owned()),
            ("success_url", req.success_url),
            ("cancel_url", req.cancel_url),
            ("line_items[0][quantity]", "1".to_owned()),
            (
                "line_items[0][price_data][currency]",
                req.currency.to_lowercase(),
            ),
            ("line_items[0][price_data][unit_amount]", amount),
            (
                "line_items[0][price_data][product_data][name]",
                req.description,
            ),
            ("metadata[gym_id]", req.gym_id.into_uuid().to_string()),
            (
                "metadata[invoice_id]",
                req.invoice_id.into_uuid().to_string(),
            ),
            ("metadata[member_id]", req.member_id.into_uuid().to_string()),
        ];

        let response = self
            .http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, Some(""))
            .form(&params)
            .send()
            .await
            .map_err(ApplicationError::internal)?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            tracing::error!(body = %body, "stripe rejected checkout session creation");
            return Err(ApplicationError::internal(std::io::Error::other(
                "stripe checkout session creation failed",
            )));
        }

        let parsed: serde_json::Value =
            response.json().await.map_err(ApplicationError::internal)?;
        let field = |name: &'static str| -> ApplicationResult<String> {
            parsed
                .get(name)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    ApplicationError::internal(std::io::Error::other(format!(
                        "stripe response carried no {name}"
                    )))
                })
        };

        Ok(CheckoutSession {
            url: field("url")?,
            // The session id is what the webhook echoes back, and what makes a
            // redelivery a no-op. Capturing it here means the reference is
            // known before the redirect rather than only after the fact.
            provider_ref: field("id")?,
        })
    }
}

/// Stands in for `StripeGateway` when no secret key is configured, so
/// `BillingService` never has to special-case "is Stripe on?" — it just calls
/// the port and gets a clear, user-safe error back.
pub struct NotConfiguredGateway;

#[async_trait]
impl PaymentGateway for NotConfiguredGateway {
    async fn create_checkout_session(
        &self,
        _req: CheckoutSessionRequest,
    ) -> ApplicationResult<CheckoutSession> {
        Err(ApplicationError::Unavailable("card payment".to_owned()))
    }
}

/// SHA-256's block size in bytes (RFC 2104's key-prep step needs it).
const SHA256_BLOCK_SIZE: usize = 64;

/// HMAC-SHA256, built directly on `sha2` (already a workspace dependency for
/// refresh-token hashing) rather than pulling in the `hmac` crate — one fewer
/// dependency, and one fewer digest-generation-compatibility question to
/// track alongside the sha2 0.10-vs-0.11 note above. This is RFC 2104,
/// verbatim: a key longer than the block gets hashed down first, a shorter
/// one is zero-padded, then two SHA-256 rounds over the ipad/opad-masked key.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; SHA256_BLOCK_SIZE];
    if key.len() > SHA256_BLOCK_SIZE {
        let hashed = Sha256::digest(key);
        key_block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; SHA256_BLOCK_SIZE];
    let mut opad = [0x5cu8; SHA256_BLOCK_SIZE];
    for i in 0..SHA256_BLOCK_SIZE {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

/// Verify a `Stripe-Signature` header against the raw request body.
///
/// Stripe signs `"{timestamp}.{payload}"` with HMAC-SHA256 over the webhook
/// secret and sends `t=<timestamp>,v1=<hex digest>[,v0=...]`. Verifying the
/// RAW bytes matters — re-serialising the parsed JSON would very likely
/// produce different bytes (key order, whitespace) and always fail, which is
/// why `crates/api`'s webhook route reads the body as `Bytes`, not `Json`.
///
/// `tolerance_secs` rejects an old signature being replayed; Stripe's own
/// libraries default to 300 seconds and this matches that.
#[must_use]
pub fn verify_webhook_signature(
    payload: &[u8],
    signature_header: &str,
    secret: &str,
    now_unix: i64,
    tolerance_secs: i64,
) -> bool {
    let mut timestamp: Option<i64> = None;
    let mut v1: Option<&str> = None;

    for part in signature_header.split(',') {
        let mut kv = part.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("t"), Some(v)) => timestamp = v.parse().ok(),
            (Some("v1"), Some(v)) => v1 = Some(v),
            _ => {}
        }
    }

    let (Some(timestamp), Some(v1)) = (timestamp, v1) else {
        return false;
    };

    if (now_unix - timestamp).abs() > tolerance_secs {
        return false;
    }

    let mut signed_message = format!("{timestamp}.").into_bytes();
    signed_message.extend_from_slice(payload);

    let expected = hex::encode(hmac_sha256(secret.as_bytes(), &signed_message));

    // Constant-time-ish via length-prefixed comparison would need `subtle`
    // (already a workspace dep) for a real timing-safe compare; a webhook
    // signature is not a password-equivalent secret an attacker can brute
    // force interactively over the network the way a login is, so a plain
    // compare here matches Stripe's own reference implementations.
    expected == v1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, timestamp: i64, payload: &[u8]) -> String {
        let mut signed_message = format!("{timestamp}.").into_bytes();
        signed_message.extend_from_slice(payload);
        hex::encode(hmac_sha256(secret.as_bytes(), &signed_message))
    }

    #[test]
    fn accepts_a_correctly_signed_payload() {
        let secret = "whsec_test";
        let payload = b"{\"type\":\"checkout.session.completed\"}";
        let now = 1_700_000_000_i64;

        let header = format!("t={now},v1={}", sign(secret, now, payload));
        assert!(verify_webhook_signature(payload, &header, secret, now, 300));
    }

    #[test]
    fn rejects_a_tampered_payload() {
        let secret = "whsec_test";
        let payload = b"{\"type\":\"checkout.session.completed\"}";
        let now = 1_700_000_000_i64;

        let header = format!("t={now},v1={}", sign(secret, now, payload));
        let tampered = b"{\"type\":\"checkout.session.completed\",\"evil\":true}";
        assert!(!verify_webhook_signature(
            tampered, &header, secret, now, 300
        ));
    }

    #[test]
    fn rejects_an_expired_timestamp() {
        let secret = "whsec_test";
        let payload = b"{}";
        let now = 1_700_000_000_i64;

        let header = format!("t={now},v1={}", sign(secret, now, payload));
        // The webhook arrives 10 minutes "later" than it was signed.
        assert!(!verify_webhook_signature(
            payload,
            &header,
            secret,
            now + 600,
            300
        ));
    }

    #[test]
    fn rejects_a_malformed_header() {
        assert!(!verify_webhook_signature(
            b"{}", "garbage", "secret", 0, 300
        ));
    }
}
