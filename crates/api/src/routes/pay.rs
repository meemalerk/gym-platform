//! The self-hosted card page (ADR-0028).
//!
//! The only routes in the API that serve HTML, and the only ones with no
//! `TenantScope` and no bearer token — because the caller is a browser
//! following a redirect, exactly as it would be at a real processor. Trust
//! comes from the **signed session token in the path**, which carries the gym,
//! the invoice, the member and the amount, so nothing can be re-pointed by
//! editing a URL.
//!
//! Server-rendered on purpose. A card page that pulled in a framework would
//! need a build step, a bundle and a CSP conversation, to render one form —
//! and the demo's whole promise is that it runs with nothing installed.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;
use gym_application::billing::GatewayPaymentCommand;
use gym_domain::{GymId, InvoiceId, UserId, billing::PaymentProvider, billing::format_money};
use gym_infrastructure::{CardOutcome, evaluate_card};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CardForm {
    pub number: String,
    pub expiry: String,
    pub cvc: String,
}

/// Show the card form.
///
/// Not in the OpenAPI document: it serves HTML to a person, not JSON to a
/// client, and listing it would put a route in the generated SDK that no
/// client should ever call.
pub async fn page(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let Some(gateway) = state.dummy_gateway.as_ref() else {
        return problem("Card payment is not enabled for this gym.");
    };

    match gateway.verify(&token) {
        Ok(claims) => Html(render_form(
            &token,
            &claims.description,
            &format_money(claims.amount_minor, &claims.currency),
            None,
        ))
        .into_response(),
        Err(_) => {
            problem("This payment link is no longer valid. Open the invoice again to start over.")
        }
    }
}

/// Take the card.
///
/// On success the invoice is credited through exactly the same service call
/// the Stripe webhook uses, so the settle rule, the part-payment arithmetic
/// and the idempotency check are shared rather than reimplemented.
pub async fn submit(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(form): Form<CardForm>,
) -> Response {
    let Some(gateway) = state.dummy_gateway.as_ref() else {
        return problem("Card payment is not enabled for this gym.");
    };

    let Ok(claims) = gateway.verify(&token) else {
        return problem(
            "This payment link is no longer valid. Open the invoice again to start over.",
        );
    };

    let amount = format_money(claims.amount_minor, &claims.currency);

    match evaluate_card(&form.number, &form.expiry, &form.cvc) {
        CardOutcome::Invalid => Html(render_form(
            &token,
            &claims.description,
            &amount,
            Some("Check the card number, expiry and CVC."),
        ))
        .into_response(),

        CardOutcome::Declined => Html(render_form(
            &token,
            &claims.description,
            &amount,
            Some("That card was declined. Try a different one."),
        ))
        .into_response(),

        CardOutcome::Approved => {
            let result = state
                .billing
                .apply_gateway_payment(GatewayPaymentCommand {
                    gym_id: GymId::from(claims.gym_id),
                    member_id: UserId::from(claims.member_id),
                    invoice_id: InvoiceId::from(claims.invoice_id),
                    amount_minor: claims.amount_minor,
                    provider: PaymentProvider::Dummy,
                    // The session reference, so a resubmitted form credits the
                    // invoice once. Same key the Stripe path uses.
                    session_id: claims.sub.clone(),
                    received_on: Utc::now().date_naive(),
                })
                .await;

            match result {
                Ok(()) => {
                    // Back to wherever the app asked to be returned to. The
                    // return URL is inside the signed token, so this cannot be
                    // turned into an open redirect by editing the request.
                    Redirect::to(&claims.return_url).into_response()
                }
                Err(error) => {
                    tracing::error!(?error, "failed to apply a demo card payment");
                    Html(render_form(
                        &token,
                        &claims.description,
                        &amount,
                        Some("Something went wrong recording that payment. Please try again."),
                    ))
                    .into_response()
                }
            }
        }
    }
}

fn problem(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(format!(
            r#"<!doctype html><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Payment</title>
{STYLE}
<main><div class="card"><h1>Payment</h1><p class="err">{}</p></div></main>"#,
            escape(message)
        )),
    )
        .into_response()
}

/// Minimal HTML escaping.
///
/// Everything interpolated below is either ours or came out of a signed token,
/// so nothing here is attacker-controlled — but an invoice description is
/// free text a gym manager typed, and "it cannot contain a `<`" is the kind of
/// assumption that stops being true quietly.
fn escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The product's design language, inlined.
///
/// Sharp corners, 2px ink rules, one red-orange accent, Archivo — matching
/// `apps/mobile/src/ui/theme.ts` (ADR-0020). A checkout page that looked
/// nothing like the app it came from would read as a phishing attempt, which
/// is the exact instinct a real payment page has to survive.
const STYLE: &str = r#"<style>
:root{--surface:#f4f3f2;--surface2:#fbfaf9;--ink:#201e1d;--mut:#6c6763;
--line:#cbc7c4;--accent:#d92b0f;--on-accent:#fff;--danger:#a81c02}
@media (prefers-color-scheme:dark){:root{--surface:#141312;--surface2:#211f1d;
--ink:#f2efed;--mut:#a09892;--line:#33302e;--accent:#ff6242;--on-accent:#14100e;
--danger:#ff8168}}
*{box-sizing:border-box}
body{margin:0;background:var(--surface);color:var(--ink);
font-family:Archivo,-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;
line-height:1.5}
main{display:flex;min-height:100vh;align-items:center;justify-content:center;padding:20px}
.card{background:var(--surface2);border:2px solid var(--ink);max-width:420px;width:100%;padding:24px}
@media (prefers-color-scheme:dark){.card{border-color:var(--line)}}
h1{font-size:26px;font-weight:800;letter-spacing:-.5px;margin:0 0 4px}
.sub{color:var(--mut);font-size:13.5px;margin:0 0 18px}
.total{border-top:2px solid var(--ink);border-bottom:2px solid var(--ink);
display:flex;justify-content:space-between;align-items:baseline;padding:12px 0;margin-bottom:20px}
@media (prefers-color-scheme:dark){.total{border-color:var(--line)}}
.total .amt{font-size:26px;font-weight:800;font-variant-numeric:tabular-nums}
label{display:block;font-size:11px;font-weight:700;letter-spacing:1.1px;
text-transform:uppercase;color:var(--mut);margin:14px 0 5px}
input{width:100%;padding:12px;font:inherit;font-size:16px;background:var(--surface);
color:var(--ink);border:1px solid var(--line)}
input:focus{outline:2px solid var(--accent);outline-offset:1px}
.row{display:flex;gap:12px}.row>div{flex:1}
button{width:100%;margin-top:22px;padding:15px;font:inherit;font-size:15px;font-weight:700;
letter-spacing:.4px;background:var(--accent);color:var(--on-accent);border:0;cursor:pointer}
button:hover{filter:brightness(.94)}
.err{background:var(--surface);border-left:4px solid var(--danger);color:var(--danger);
padding:10px 12px;font-size:13.5px;margin:0 0 16px}
.hint{margin-top:20px;padding-top:14px;border-top:1px solid var(--line);
color:var(--mut);font-size:12px}
.hint code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--ink)}
.demo{display:inline-block;background:var(--accent);color:var(--on-accent);
font-size:10px;font-weight:700;letter-spacing:.8px;text-transform:uppercase;padding:2px 6px}
</style>"#;

fn render_form(token: &str, description: &str, amount: &str, error: Option<&str>) -> String {
    let banner = error.map_or_else(String::new, |e| {
        format!(r#"<p class="err">{}</p>"#, escape(e))
    });

    format!(
        r#"<!doctype html><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Pay {amount}</title>
{STYLE}
<main>
  <form class="card" method="post" action="/pay/{token}">
    <span class="demo">Demo payment</span>
    <h1>Card details</h1>
    <p class="sub">{description}</p>

    <div class="total"><span>Total</span><span class="amt">{amount}</span></div>

    {banner}

    <label for="number">Card number</label>
    <input id="number" name="number" inputmode="numeric" autocomplete="cc-number"
           placeholder="4242 4242 4242 4242" required autofocus>

    <div class="row">
      <div>
        <label for="expiry">Expiry</label>
        <input id="expiry" name="expiry" autocomplete="cc-exp" placeholder="12/30" required>
      </div>
      <div>
        <label for="cvc">CVC</label>
        <input id="cvc" name="cvc" inputmode="numeric" autocomplete="cc-csc"
               placeholder="123" required>
      </div>
    </div>

    <button type="submit">Pay {amount}</button>

    <p class="hint">
      No money moves and no card is stored — this page is served by the gym app itself.
      Any well-formed number is approved; <code>4000 0000 0000 0002</code> is declined,
      so the refusal path can be shown too.
    </p>
  </form>
</main>"#,
        description = escape(description),
        amount = escape(amount),
        token = escape(token),
    )
}
