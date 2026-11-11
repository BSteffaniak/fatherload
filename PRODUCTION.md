# Browser production hosting

Fatherload's browser build is static: Cloudflare Pages serves HTML, JS and WASM at
`https://fatherload.hyperchad.dev`. No Fly Machine, database, account login, or
server volume is required. Browser saves remain origin-specific and are not
uploaded to Cloudflare. Localhost/LAN saves do not appear on the production origin.

## CI/CD

`.github/workflows/web.yml` builds pull requests and `master` using `flake.lock`'s
Emscripten/Rust toolchain. It packages content-addressed JS/WASM, runs strict web
Clippy, deployment-tool unit tests and real Chromium acceptance against the exact
release directory, then uploads that directory as the `fatherload-web` artifact.

On `master`, the deploy job waits for the `Production` environment approval. It
uses the downloaded artifact (no rebuild), a lockfile-pinned Wrangler CLI, and
serialized production deployments. It provisions the Pages project if absent,
deploys, and attaches the custom domain/CNAME. Conflicting DNS records are refused,
not overwritten. No PWMTF/WWMTF resource is modified. Live checks verify revision,
asset hashes, MIME/cache/security headers, followed by browser acceptance on HTTPS.
The first release may wait several minutes for DNS/certificate activation.

## One-time credentials

Run from this repository with authenticated `gh`:

```sh
# Supply CLOUDFLARE_PROVISION_TOKEN via a secure credential manager/environment.
# Do not put the value in command arguments, source files, or chat.
python3 scripts/provision-production.py
```

The script creates `Production`, requires BSteffaniak approval (self-approval is
allowed for this single-maintainer repository), and restricts deployment to the
`master` branch. It uploads only missing secrets:

- `CLOUDFLARE_ACCOUNT_ID`: account containing `hyperchad.dev`.
- `CLOUDFLARE_API_TOKEN`: dedicated Fatherload deployment token with account-level
  **Pages Write**, and **Zone Read / DNS Write** for `hyperchad.dev`.

The parent provisioning token needs **API Tokens Read/Edit**, Zone Read, and
permission to delegate those project permissions. The account Pages permission
covers Pages projects in that account; Cloudflare's token scope is account-level,
not specific to the Fatherload Pages project. The zone DNS permission likewise
covers the zone; the deployment script changes only Fatherload's exact record.
The parent token is never installed in GitHub. Secret values are passed to `gh`
over stdin with subprocess output suppressed. Existing project tokens aren't
revoked. If a write is uncertain, inspect provider/GitHub state before retrying.
PWMTF's existing GitHub secrets cannot be read back or copied by the script.

Alternatively, configure that same protected environment and secrets manually.
An authenticated Fly CLI alone cannot authorize Cloudflare Pages or DNS changes.

## Local release qualification

```sh
nix develop .#web --command bash scripts/build-web.sh
python3 scripts/package-web.py --destination target/web-release-$(git rev-parse --short HEAD)
npm ci --prefix web
(cd web && npx --no-install playwright install chromium)
FATHERLOAD_WEB_ROOT=target/web-release-$(git rev-parse --short HEAD) npm test --prefix web
python3 scripts/test-deployment.py
```

The release destination must be empty; packaging never deletes existing output.
For live acceptance, use `FATHERLOAD_WEB_URL=https://fatherload.hyperchad.dev`.
Headless audio-unlock checks do not establish physical speaker/controller quality.

## Caching, security and rollback

HTML/manifest revalidate; content-addressed JS/WASM cache immutably. `locateFile`
selects the matching WASM for each release. `_headers` adds nosniff, referrer and
framing restrictions without disabling the inline JS/WebAssembly required by the
current loader. `404.html` prevents Pages' default SPA fallback returning HTML
for missing assets. No service worker or shared-memory threads are used.

Cloudflare retains previous Pages deployments for rollback. Use the Pages dashboard
to roll production back to a known successful deployment; verify `release.json`
and run the live/browser smoke tests afterward. This does not roll browser saves
back. For a normal forward release, push `master` or dispatch the workflow on
`master`, inspect build evidence, then approve the production job in GitHub.
