# Cloudflare API Cache Rules

The API already returns short-lived cache headers and the origin compresses JSON
responses with gzip. Configure the following Cloudflare Cache Rule for the
`api.lumenlp.xyz` zone to enable edge caching for read-only analytics.

## Rule

Create a rule under **Caching > Cache Rules**:

```text
http.host eq "api.lumenlp.xyz"
and http.request.method eq "GET"
and (
  starts_with(http.request.uri.path, "/v1/pools")
  or starts_with(http.request.uri.path, "/v1/lp/leaders")
)
```

Set:

- **Cache eligibility:** Eligible for cache
- **Edge TTL:** 60 seconds
- **Browser TTL:** Respect existing headers
- **Query string:** Include all query string parameters in the cache key

The query-string requirement is important: different pool filters, pagination,
leader windows, and sort modes must not share a cached response.

## Safety

Do not cache `POST`, `PATCH`, or `DELETE` requests. Do not apply this rule to
wallet, copy-session, transaction, or health endpoints. The API responses are
public analytics only and are refreshed on a one-minute cadence.

## Verification

After publishing the rule, run:

```bash
curl -sSI 'https://api.lumenlp.xyz/v1/lp/leaders?window_days=1&limit=24'
```

Expected headers after the first request is stored at the edge:

```text
cf-cache-status: HIT
cache-control: public, max-age=60, s-maxage=60, stale-while-revalidate=120
```

The first request may show `MISS`; a second request with the identical URL
should show `HIT`.
