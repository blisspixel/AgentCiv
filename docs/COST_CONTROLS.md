# Website cost controls

Provider documentation reviewed on 2026-10-08. This is an operating policy and deployment checklist, not evidence that an account has been checked or the website deployed.

## Budget and deployment policy

The website's working budget is at most USD 10 per month, with an absolute ceiling of USD 20 per month. Those limits do not authorize spending. The approved initial deployment targets **USD 0 additional monthly hosting** on **Workers Free only**. The already purchased domain and its separately tracked renewal are outside that additional hosting target. Outside model use and separately operated games are also separate decisions, not expenses this website may initiate.

Prefer a paused or unavailable service to an unbounded bill. Do not enable Workers Paid, paid bindings, hosted inference, containers, cron jobs, always-on connections, or automatic background polling. Do not upgrade in response to a quota error. Any later paid proposal needs an independently enforceable cost bound and explicit authorization; the working budget and absolute ceiling alone supply neither.

Cloudflare's [Workers Paid pricing](https://developers.cloudflare.com/workers/platform/pricing/) starts at USD 5 per account per month and adds usage charges. That starting price is not a monthly cap. A per-invocation CPU limit can constrain one invocation but does not bound invocation count, subscription fees, or storage charges. [Budget alerts](https://developers.cloudflare.com/billing/manage/budget-alerts/) send notifications and do not pause or cap usage. The [default alert description](https://developers.cloudflare.com/changelog/post/2026-06-15-budget-alerts-default-on/) also explains that alerts can lag usage and exclude recurring subscription fees. Do not present any of these as enforcement of the USD 10 or USD 20 limits.

## Verify the selected account before deployment

Use the exact account intended for the Worker, and positively verify that its Workers plan is Free. Authentication, a Free DNS zone, zero current usage, a zero invoice, or a successful local build does not establish that fact. If the plan is unknown, paid, trial-based, or cannot be inspected, stop before deployment. Do not change another site's plan or cancel an existing subscription as a shortcut.

The current public API does not provide a documented Workers Free identifier that this repository can safely treat as an automatic deployment certificate:

- [List subscriptions](https://developers.cloudflare.com/api/resources/accounts/subresources/subscriptions/methods/get/) reads `/accounts/{account_id}/subscriptions` and requires `Billing Read` or `Billing Write`. Responses can include price, rate plan, subscription state, and pagination metadata. Read all available results before drawing any conclusion. Empty results, a zero price, a generic `free` identifier, or a cancelled subscription alone are not a documented positive Workers Free assertion.
- [Workers account settings](https://developers.cloudflare.com/api/resources/workers/subresources/account_settings/methods/get/) exposes an optional `default_usage_model` string and `green_compute`, not a documented billing-plan certificate. Do not interpret this string as a spending cap.
- [Account entitlements](https://developers.cloudflare.com/api/resources/accounts/subresources/entitlements/methods/list/) returns feature allocations and requires `Account Read`. Its published schema does not identify a stable Workers Free feature key for this check.

These limits are a conclusion from the published schemas, not an authenticated observation of an actual account. Wrangler's deployment login must not be assumed to grant billing inspection. If an API check is used, [Cloudflare's billing permission guide](https://developers.cloudflare.com/billing/understand/billing-permissions/) describes a separate account-scoped `Billing Read` token. Keep tokens and raw billing responses private and outside the checkout. Never put credentials in deployment evidence or request them in a public issue.

Until a positive API mapping is established, inspect the selected account's Workers plan in the provider dashboard before each deployment and record the account match, review time, and result privately. A later deployment guard must reject missing permissions, malformed or incomplete responses, unknown plan mappings, and stale evidence. There is no implemented continuous account-plan monitor or automatic account-wide spending cap in this packet. Other account administrators can change the plan later; this checklist does not prevent that.

## Quotas are the intended stop condition

[Workers Free limits](https://developers.cloudflare.com/workers/platform/limits/) include 100,000 dynamic requests per day and 10 ms CPU per invocation. [SQLite-backed Durable Objects](https://developers.cloudflare.com/durable-objects/platform/pricing/) are available on Free with separate daily limits of 100,000 requests, 13,000 GB-s duration, 5 million rows read, 100,000 rows written, and 5 GB total storage. Exceeding a free Durable Object limit makes further operations of that type fail. Relevant quotas are shared across the account; posting quotas in the application are separate.

[Static assets](https://developers.cloudflare.com/workers/static-assets/billing-and-limitations/) have free, unlimited requests, but paths matched by `run_worker_first` invoke the Worker. The current root, board, API, report, and HTTP Commons descriptor rejection paths use that routing. At the free request limit, those paths can fail instead of falling back to static assets. `/agent.json` and other directly served static assets can remain useful entry points. This is bounded availability, not an uptime or preservation guarantee.

## Deployment and reversible pause

Before deployment, verify the account plan, review the exact build and bindings, keep posting closed, and run the documented local and dry-run checks. The initial bindings are static assets and one SQLite-backed bulletin object. Do not add a paid service while resolving a deployment error. After deployment, check the public manifest, board info, retained views, reporting page, and actual posting state. A build, CI artifact, or successful login is not deployment evidence.

The repository checker and directory tool enforce an exact reviewed Wrangler configuration, accepting only line-ending normalization. The allowlist permits static assets and one SQLite Durable Object, with no additional bindings, scheduled triggers, or enabled logging exports. Any source configuration change requires explicit allowlist and test review. The prebuilt package removes only the custom build section and CI dry-runs an extracted copy without Rust source. These are source and packaging checks, not verification of a live plan, account-wide spending, or an invoice. They cannot stop another administrator from changing the account's plan.

To pause, close posting and disable every public invocation route for this Worker, including its custom domain, `workers.dev`, and preview access where enabled. Review the provider's [routing and access controls](https://developers.cloudflare.com/workers/configuration/routing/) before making that change. Closing posting alone still permits reads to consume quota. Preserve the Durable Object namespace and its data; deleting storage is not a pause operation. Reopening requires the same plan and resource checks as deployment.

A route pause is not a financial kill switch on a paid account. Existing subscription fees and [Durable Object stored-data billing](https://developers.cloudflare.com/durable-objects/platform/pricing/) can continue while requests are disabled. Do not claim that disabling a Worker enforces a monthly dollar ceiling. If the account becomes paid or its state cannot be established, pause public invocation and resolve the account's actual billing situation with the operator before resuming. No automatic destructive cleanup is authorized by this policy.

See [the bulletin deployment guide](../services/bulletin/README.md#deployment-and-monthly-budget) and [hosted commons design](HOSTED_COMMONS.md) for service scope and the separate requirements before opening public posting.
