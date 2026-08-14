import Link from "next/link";

export default function HomePage() {
  return (
    <main className="landing">
      <section className="landing-hero">
        <div className="landing-eyebrow">Stellar LP automation</div>
        <h1 className="landing-title">
          Track LPs. Copy the best. <span className="text-gradient">Automate the rest.</span>
        </h1>
        <p className="landing-lead">
          Discover active Aquarius LPs, set your copy rules and limits, and let
          policy-controlled automation execute approved liquidity actions.
        </p>
        <div className="landing-actions">
          <Link className="btn-solid" href="/leaders">
            Find a Leader
          </Link>
          <Link className="btn-ghost" href="/pools">
            Explore pools
          </Link>
        </div>
        <div className="protocol-row">
          <span className="protocol-pill">Aquarius</span>
          <span className="protocol-pill">Soroban</span>
          <span className="protocol-pill">Stellar</span>
        </div>
      </section>

      <section className="acts" aria-label="Product acts">
        <div className="acts-intro">
          <div className="landing-eyebrow">How it works</div>
          <h2 className="acts-heading">Discover first. Copy with context.</h2>
          <p className="acts-lead">
            LumenLP turns on-chain LP activity into an automated workflow: find a
            Leader, define the rules, and let the policy handle approved actions.
          </p>
        </div>

        <article className="act-row">
          <div className="act-copy">
            <div className="act-index" aria-hidden="true">01</div>
            <div className="act-label">Act 01 · Discover</div>
            <h3 className="act-title">Find LPs with real on-chain activity</h3>
            <p className="act-body">
              Compare claimed fees, deposits, withdrawals, pools touched, and
              current exposure. Rankings show observable data so you can make
              your own decision.
            </p>
            <Link className="act-link" href="/leaders">
              Browse Leaders →
            </Link>
          </div>
          <div className="act-visual" aria-hidden="true">
            <div className="act-chip-row">
              <span className="act-chip active">Claimed fees</span>
              <span className="act-chip">Activity</span>
              <span className="act-chip">Exposure</span>
            </div>
            <div className="act-step">1 · Select a Leader</div>
            <div className="act-step">2 · Inspect LP activity</div>
            <div className="act-step muted">3 · Choose who to copy</div>
          </div>
        </article>

        <article className="act-row reverse">
          <div className="act-copy">
            <div className="act-index" aria-hidden="true">02</div>
            <div className="act-label">Act 02 · Copy</div>
            <h3 className="act-title">Copy LP actions, not random swaps</h3>
            <p className="act-body">
              Follow Aquarius deposits, withdrawals, and fee claims at a
              coefficient you choose. Each operation stays tied to its source
              event and pool.
            </p>
            <Link className="act-link" href="/copy">
              Start a copy session →
            </Link>
          </div>
          <div className="act-visual" aria-hidden="true">
            <div className="act-visual-bar">
              <span>Leader deposit</span>
              <span className="metric-positive">× 10%</span>
            </div>
            <div className="act-visual-bar dim">
              <span>AQUA / XLM</span>
              <span>scaled action</span>
            </div>
            <div className="act-visual-bar dim">
              <span>Source event</span>
              <span>verified on-chain</span>
            </div>
            <div className="act-visual-spark">
              <span style={{ height: "38%" }} />
              <span style={{ height: "62%" }} />
              <span style={{ height: "48%" }} />
              <span style={{ height: "78%" }} />
              <span style={{ height: "56%" }} />
              <span style={{ height: "88%" }} />
              <span style={{ height: "70%" }} />
            </div>
          </div>
        </article>

        <article className="act-row">
          <div className="act-copy">
            <div className="act-index" aria-hidden="true">03</div>
            <div className="act-label">Act 03 · Control</div>
            <h3 className="act-title">Set the rules once. Automate within limits.</h3>
            <p className="act-body">
              Choose a copy size, pool allowlist, and per-operation or daily
              limits. The policy-controlled execution layer handles approved
              actions while you keep full control.
            </p>
            <Link className="act-link" href="/copy">
              Configure automation →
            </Link>
          </div>
          <div className="act-visual" aria-hidden="true">
            <div className="act-stat-grid">
              <div className="act-stat">
                <span className="act-stat-label">Copy size</span>
                <strong>10%</strong>
              </div>
              <div className="act-stat">
                <span className="act-stat-label">Policy</span>
                <strong>Active</strong>
              </div>
              <div className="act-stat">
                <span className="act-stat-label">Daily cap</span>
                <strong className="metric-positive">Set</strong>
              </div>
              <div className="act-stat">
                <span className="act-stat-label">Execution</span>
                <strong>Guarded</strong>
              </div>
            </div>
          </div>
        </article>

        <article className="act-row reverse" id="api">
          <div className="act-copy">
            <div className="act-index" aria-hidden="true">04</div>
            <div className="act-label">Act 04 · Build</div>
            <h3 className="act-title">Build on Stellar LP activity data</h3>
            <p className="act-body">
              Use pool metrics, historical events, Leader activity, and positions
              to build your own LP dashboard, strategy, or copy workflow.
            </p>
            <a className="act-link" href="#api-sample">
              See sample request →
            </a>
          </div>
          <div className="act-visual codey" aria-hidden="true">
            <div className="act-code-line">
              <span className="tok-key">GET</span> /v1/pools
            </div>
            <div className="act-code-line dim">/v1/pools/{"{address}"}/history</div>
            <div className="act-code-line dim">/v1/positions?owner=G…</div>
            <div className="act-code-line dim">/v1/positions/summary</div>
          </div>
        </article>
      </section>

      <section className="landing-api" id="api-sample" aria-label="API sample">
        <div className="panel-head">GET /v1/pools</div>
        <pre>{`const res = await fetch("https://api.lumenlp.xyz/v1/pools");
const { pools } = await res.json();
// fee/TVL, flow, scores — inputs for your LP app or copy workflow`}</pre>
      </section>

      <section className="landing-close">
        <h2 className="acts-heading">Stop manually babysitting your LP strategy.</h2>
        <p className="acts-lead" style={{ marginBottom: 16 }}>
          Track on-chain activity, configure your limits, and automate the
          liquidity actions you approve.
        </p>
        <div className="landing-actions">
          <Link className="btn-solid" href="/leaders">
            Find a Leader
          </Link>
          <Link className="btn-ghost" href="/pools">
            Explore pools
          </Link>
        </div>
      </section>
    </main>
  );
}
