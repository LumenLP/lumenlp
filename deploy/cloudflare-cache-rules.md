# Cloudflare API Cache Rules

The API already returns short-lived cache headers and the origin compresses JSON
responses with gzip. Configure the following Cloudflare Cache Rule for the
`api.lumenlp.xyz` zone to enable edge caching for read-only analytics.

## Rule

In the Cloudflare dashboard, select the `lumenlp.xyz` zone and create a rule
under **Caching > Cache Rules > Create rule**:

```text
http.host eq "api.lumenlp.xyz"
and http.request.method eq "GET"
and (
  starts_with(http.request.uri.path, "/v1/pools")
  or starts_with(http.request.uri.path, "/v1/lp/leaders")
  or starts_with(http.request.uri.path, "/v1/lp/profile")
)
```

Set:

- **Cache eligibility:** Eligible for cache
- **Edge TTL:** 30 seconds
- **Browser TTL:** Respect existing headers
- **Query string:** Include all query string parameters in the cache key

The query-string requirement is important: different pool filters, pagination,
leader windows, sort modes, and profile addresses must not share a cached
response.

## Safety

Do not cache `POST`, `PATCH`, or `DELETE` requests. Do not apply this rule to
wallet, copy-session, transaction, or health endpoints. The API responses are
public analytics only and are refreshed on a one-minute cadence. Profile
responses are keyed by the requested public Stellar address and contain no
private wallet data or authentication state.

## Verification

After publishing the rule, run:

```bash
curl -sSI 'https://api.lumenlp.xyz/v1/lp/leaders?window_days=1&limit=24'
```

Expected headers after the first request is stored at the edge:

```text
cf-cache-status: HIT
cache-control: public, max-age=30, s-maxage=30, stale-while-revalidate=30
cdn-cache-control: public, max-age=30, stale-while-revalidate=30
```

The first request may show `MISS`; a second request with the identical URL
should show `HIT`.
