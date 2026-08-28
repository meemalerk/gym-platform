# Hosting & Deployment

> July-2026 research snapshot — prices move; re-verify at deploy time. Companion:
> [cost-analysis.md](cost-analysis.md) (the full money model) and
> [tech-stack.md](tech-stack.md). Bootstrapping bias: cheapest-viable, self-manage early,
> buy managed only when ops burden justifies it.

## TL;DR recommended setup

- **Compute:** Hetzner Cloud VPS (cheapest by a wide margin) + Docker Compose + Caddy (TLS) +
  **Kamal** for near-zero-downtime deploys. **No Kubernetes early** — it's not worth it until
  well into growth stage.
- **Postgres:** self-managed on a Hetzner box pre-launch; move to **managed HA (DO Managed PG
  ~$60/mo or Neon)** at the first paying-customer-with-SLA milestone.
- **AI inference:** **do NOT run a GPU early** — cheap serverless open-model APIs are cheaper at
  our scale (see [cost-analysis.md](cost-analysis.md) and the ADR-0011 update). Rent a GPU
  (RunPod RTX 4090 ~$248/mo) only at high, steady volume or for data-control reasons.
- **Object storage:** **Cloudflare R2** (zero egress — kills the biggest cost-surprise for a
  media app).
- **IaC:** OpenTofu (or hand-provision 1–2 boxes until you have 3+ environments).

## Compute hosting (Rust backend + worker)

| Option | ~2 vCPU / 4 GB monthly | When |
|--------|----------------------:|------|
| **Hetzner CX22** (shared Intel) | **~$5** | Default. Cheapest good option. |
| Hetzner CPX (AMD, more traffic) | ~$23 | When you want more headroom |
| AWS Lightsail | ~$20–24 | Flat-rate, AWS ecosystem, low effort |
| DigitalOcean Droplet | ~$24 | Simple, per-second billing |
| Railway | ~$20–40 | PaaS convenience, usage-based |
| Fly.io (perf-2x) | ~$64 | PaaS, per-second, global |
| Google Cloud Run | ~$50–70 | Scale-to-zero-ish, GCP |
| Render (Pro 2CPU/4GB) | ~$85 + $25 workspace | Most expensive here |
| AWS Fargate | ~$67 compute, **~$100–150 real** (ALB/NAT) | Avoid early — hidden costs |

**Hetzner is ~5–15× cheaper** than the US PaaS players for equivalent specs — the tradeoff is
you run your own OS/Docker/reverse-proxy/TLS/monitoring. That's fine for a bootstrapping team.
Lightsail is the best low-effort non-Hetzner flat-rate option.

> ⚠ **Hetzner raised Cloud prices twice in 2026** (Apr + Jun); the new CX line is currently the
> cheapest tier. Verify at deploy time.

## PostgreSQL

| Option | Free tier | Small-prod (HA-ish) |
|--------|-----------|--------------------:|
| **Self-managed on Hetzner VPS** | — | **$5–25/mo** (box, maybe shared with app) |
| Neon | 100 CU-hrs, 0.5 GB | ~$15–30/mo (no monthly minimum now) |
| Supabase | 500 MB, pauses | **$25/mo** Pro (8 GB DB) |
| DigitalOcean Managed PG | — | single $15 → **HA $60/mo** |
| Crunchy Bridge | — | ~$30–60/mo prod |
| AWS RDS | — | Multi-AZ real total ~$60–100/mo |

**Self-managed is 5–20× cheaper** but you own backups/patching/failover. **You do NOT need a
DBA to self-host early** — `pg_dump` cron + WAL archiving + a monitoring alert goes a long way.
**Go managed** once DB-ops would exceed ~10% of someone's time or you have real SLA/uptime
expectations; managed HA + PITR then costs only ~$15–60/mo more and removes a major single-team
risk. **Rule:** self-host through pre-launch/early; migrate to managed at first
paying-customer-with-SLA.

pgvector (for RAG — see [ADR-0011](adr/0011-self-hosted-open-llm.md)) runs in the same Postgres;
no separate vector DB.

## AI inference hosting

**The cost-optimal choice at our scale is serverless open-model APIs, not a self-hosted GPU** —
full analysis in [cost-analysis.md](cost-analysis.md). Summary:

- **Serverless open-model** (DeepInfra / Together / OpenRouter) for Qwen3.5-9B-class:
  **~$0.05–0.25 per million tokens** → a few dollars/month at early scale. Breakeven vs a
  dedicated GPU is **~1–4 billion tokens/month** — far beyond early usage.
- **Self-hosted GPU** (RunPod RTX 4090 **~$248/mo** 24/7, or **~$75–150/mo** business-hours
  scheduled with cold-start; Hetzner GEX44 flat **~$200/mo** but only 20 GB VRAM) — choose only
  at high steady volume **or** for the data-control/privacy reason (health data never leaves
  our infra).
- ⚠ **Fly.io is deprecating GPU hosting (2026-07-31)** — do not build on it.

## Object storage / CDN

- **Cloudflare R2** — **$0.015/GB, zero egress.** Default choice; S3-compatible. Zero egress
  removes the biggest cost surprise for a video/photo-heavy app.
- Backblaze B2 ($0.006/GB) is cheaper storage; front it with Cloudflare/Bunny for free egress.
- **Avoid S3 direct-to-internet egress** ($0.09/GB) — always front S3 with CloudFront if used.

## Supporting services (free tiers get you far)

| Service | Free | First paid |
|---------|------|-----------:|
| Resend (email) | 3,000/mo | $20/mo (50k) |
| Expo push | always free | EAS build/update tiers later |
| Sentry | 5k errors/mo | ~$26–29/mo |
| OpenFGA | self-host $0 (shares our Postgres) | Okta FGA = enterprise/custom |
| Infisical (secrets) | 5 identities | $18/identity/mo (self-hostable) |
| GitHub Actions | 2,000 Linux min/mo | $0.006/min |

## Deployment approach

- **Docker Compose on 1–3 Hetzner VPS** for all early stages — app + worker + OpenFGA
  container (+ optionally self-hosted Postgres) in one `docker-compose.yml`. Minimal surface.
- **Kamal** (37signals' Docker deploy tool) or systemd+Compose for **near-zero-downtime**
  rolling deploys; two app containers behind Caddy/Nginx or a Hetzner Load Balancer (~$5–6/mo).
- **No Kubernetes** until multiple independently-scaling services + someone who operates k8s —
  premature for a bootstrapping gym SaaS; many companies never need it.
- **IaC:** OpenTofu (Terraform fork) with the hcloud provider, or Pulumi if the team prefers
  real code. Hand-provisioning 1–2 boxes is fine until 3+ environments.

## Three-stage infra cost (see [cost-analysis.md](cost-analysis.md) for the full model)

| Stage | Monthly infra |
|-------|--------------:|
| Pre-launch / MVP (free tiers) | **~$0–25** |
| Early (a few gyms, hundreds of members) | **~$100–450** (lower end if serverless AI; GPU is the swing) |
| Growth (dozens of gyms, thousands of members) | **~$900–2,500+** |
