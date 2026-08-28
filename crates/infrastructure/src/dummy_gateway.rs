//! A card gateway this deployment hosts itself (ADR-0028).
//!
//! Not a test double. It implements the same `PaymentGateway` port Stripe does,
//! serves its own hosted checkout page, and everything downstream of it — the
//! payment row, the settle, the idempotency key — is the production path. Only
//! the bank is replaced, by a rule about which card numbers succeed.
//!
//! It exists because a Stripe key is a real commercial account: it cannot ship
//! in a clone-and-run demo and it cannot sit in a verification suite. Without
//! this, "pay an invoice" was the one significant flow in the product with no
//! executable proof, in a codebase whose whole discipline is executable proof.
//!
//! The session token is signed with the same secret as everything else but
//! carries its own `purpose` claim, so a token minted for one thing can never
//! be accepted by another — the same defence `checkin_pass` uses, and for the
//! same reason.

use async_trait::async_trait;
use chrono::Utc;
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{CheckoutSession, CheckoutSessionRequest, PaymentGateway},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The one value `purpose` may hold here.
const PURPOSE: &str = "dummy_checkout";

/// How long a checkout page stays valid. Long enough to find a card, short
/// enough that a link pasted into a chat is useless tomorrow.
const SESSION_TTL_SECONDS: i64 = 1800;

/// What the signed session token carries.
///
/// The amount is IN the token, and that is the point: the page cannot be
/// re-pointed at a different invoice or a different sum by editing a query
/// string, because every field it needs is inside something signed.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckoutClaims {
    /// The gateway's own reference for this attempt — the idempotency key.
    pub sub: String,
    pub gym_id: Uuid,
    pub invoice_id: Uuid,
    pub member_id: Uuid,
    pub amount_minor: i64,
    pub currency: String,
    pub description: String,
    pub return_url: String,
    purpose: String,
    exp: i64,
}

pub struct DummyGateway {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    /// Where this API is reachable from a browser. The checkout URL has to be
    /// absolute — it is handed to a browser, not fetched by us — so a wrong
    /// value here produces a link that 404s for everyone except the person who
    /// configured it. Hence the explicit setting rather than a guess.
    public_base_url: String,
}

impl DummyGateway {
    #[must_use]
    pub fn new(secret: &[u8], public_base_url: String) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.required_spec_claims = ["exp", "sub"].into_iter().map(String::from).collect();

        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            validation,
            public_base_url: public_base_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Verify a token presented back to the pay page.
    ///
    /// Every failure — bad signature, expired, minted for another purpose —
    /// collapses to one error. Whoever is holding a broken link has no use for
    /// the distinction, and telling them narrows a guess.
    pub fn verify(&self, token: &str) -> ApplicationResult<CheckoutClaims> {
        let claims = decode::<CheckoutClaims>(token, &self.decoding, &self.validation)
            .map_err(|_| invalid_session())?
            .claims;

        if claims.purpose != PURPOSE {
            return Err(invalid_session());
        }

        Ok(claims)
    }
}

fn invalid_session() -> ApplicationError {
    ApplicationError::Domain(gym_domain::DomainError::Invalid(
        "This payment link is no longer valid. Open the invoice again to start over.".to_owned(),
    ))
}

#[async_trait]
impl PaymentGateway for DummyGateway {
    async fn create_checkout_session(
        &self,
        req: CheckoutSessionRequest,
    ) -> ApplicationResult<CheckoutSession> {
        // A fresh reference per attempt, exactly like a Stripe session id. It
        // is what makes a resubmitted form a no-op rather than a second charge.
        let reference = format!("dummy_{}", Uuid::now_v7().simple());

        let claims = CheckoutClaims {
            sub: reference.clone(),
            gym_id: req.gym_id.into_uuid(),
            invoice_id: req.invoice_id.into_uuid(),
            member_id: req.member_id.into_uuid(),
            amount_minor: req.amount_minor,
            currency: req.currency,
            description: req.description,
            return_url: req.success_url,
            purpose: PURPOSE.to_owned(),
            exp: Utc::now().timestamp() + SESSION_TTL_SECONDS,
        };

        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(ApplicationError::internal)?;

        Ok(CheckoutSession {
            url: format!("{}/pay/{token}", self.public_base_url),
            provider_ref: reference,
        })
    }
}

/// Which test cards do what.
///
/// Deliberately tiny and deliberately documented on the page itself. A demo
/// gateway whose failure modes are a secret is only half a demo — the
/// interesting thing to show is not that payment works, it is what the app
/// does when it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardOutcome {
    Approved,
    Declined,
    /// Right card, wrong expiry or CVC — the everyday mistake, and worth
    /// distinguishing so the page can say something more useful than "no".
    Invalid,
}

/// Decide what a submitted card does.
///
/// Digits only after stripping spaces, because people type them in groups.
#[must_use]
pub fn evaluate_card(number: &str, expiry: &str, cvc: &str) -> CardOutcome {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();

    if digits.len() < 12 || cvc.len() < 3 || !expiry.contains('/') {
        return CardOutcome::Invalid;
    }

    // One number always declines, so the unhappy path is demonstrable. Anything
    // else well-formed is approved — this takes no money, so there is nothing
    // to protect and every reason to make the happy path easy to reach.
    if digits.ends_with("0002") {
        return CardOutcome::Declined;
    }

    CardOutcome::Approved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_card_is_approved() {
        assert_eq!(
            evaluate_card("4242 4242 4242 4242", "12/30", "123"),
            CardOutcome::Approved
        );
    }

    #[test]
    fn spaces_are_ignored_because_people_type_them() {
        assert_eq!(
            evaluate_card("4242424242424242", "12/30", "123"),
            CardOutcome::Approved
        );
    }

    #[test]
    fn the_decline_card_declines() {
        assert_eq!(
            evaluate_card("4000 0000 0000 0002", "12/30", "123"),
            CardOutcome::Declined
        );
    }

    #[test]
    fn a_short_number_is_invalid_not_declined() {
        // Different outcomes on purpose: "your card was refused" and "you
        // mistyped it" want different words on the screen.
        assert_eq!(evaluate_card("4242", "12/30", "123"), CardOutcome::Invalid);
    }

    #[test]
    fn a_missing_cvc_or_expiry_is_invalid() {
        assert_eq!(
            evaluate_card("4242424242424242", "12/30", "1"),
            CardOutcome::Invalid
        );
        assert_eq!(
            evaluate_card("4242424242424242", "1230", "123"),
            CardOutcome::Invalid
        );
    }

    #[test]
    fn a_token_round_trips_and_a_foreign_one_does_not() {
        let gateway = DummyGateway::new(b"secret-for-tests", "http://localhost:8080/".to_owned());

        // A token signed with the same key but a different purpose must not be
        // accepted — the guard that stops an access token being spent.
        #[derive(Serialize)]
        struct Other {
            sub: String,
            purpose: String,
            exp: i64,
        }
        let foreign = encode(
            &Header::new(Algorithm::HS256),
            &Other {
                sub: "x".to_owned(),
                purpose: "gym_entry".to_owned(),
                exp: Utc::now().timestamp() + 600,
            },
            &EncodingKey::from_secret(b"secret-for-tests"),
        )
        .unwrap();

        assert!(gateway.verify(&foreign).is_err());
        assert!(gateway.verify("not-a-token").is_err());
    }

    #[test]
    fn the_base_url_loses_its_trailing_slash() {
        // Otherwise every checkout url gains a double slash, which works
        // everywhere except the one proxy that normalises it into a redirect.
        let gateway = DummyGateway::new(b"k", "http://localhost:8080/".to_owned());
        assert_eq!(gateway.public_base_url, "http://localhost:8080");
    }
}
