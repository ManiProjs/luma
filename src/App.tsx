const base = import.meta.env.BASE_URL
const repo = 'https://github.com/ManiProjs/luma'

function Brand() {
  return (
    <a className="brand" href={base} aria-label="Luma home">
      <img src={`${base}luma-mark.svg`} alt="" />
      <span>LUMA</span>
    </a>
  )
}

function Arrow() {
  return <span aria-hidden="true" className="arrow">↗</span>
}

function App() {
  return (
    <div className="site-shell">
      <header className="header wrap">
        <Brand />
        <nav aria-label="Main navigation">
          <a href="#products">Products</a>
          <a href="#loop">The loop</a>
          <a href={`${base}docs/`}>Documentation</a>
        </nav>
        <a className="header-cta" href={repo} target="_blank" rel="noreferrer">Explore the source <Arrow /></a>
      </header>

      <main>
        <section className="hero wrap">
          <div className="hero-meta"><span>AN OPEN-SOURCE AGENT WORKSPACE</span><span>BUILT IN RUST · YOUR MODEL, YOUR RULES</span></div>
          <div className="hero-main">
            <div className="hero-copy">
              <h1>Less yak-shaving.<br /><span>More shipping.</span></h1>
              <p className="hero-lede">A workspace for getting real work out of AI agents. Pair up with Code, or hand the long-running stuff to Teammates. Either way, keep your hands on the wheel.</p>
              <div className="hero-actions">
                <a className="button button-dark" href={`${repo}/releases`}>Get Luma <Arrow /></a>
                <a className="text-link" href={`${base}docs/`}>Read the field guide <span>→</span></a>
              </div>
            </div>
            <div className="hero-art" aria-label="The Luma mark, two overlapping circles in blue and orange" role="img">
              <div className="art-index">L / 001</div>
              <img src={`${base}luma-mark.svg`} alt="" />
              <span className="art-caption">A LITTLE LESS BUSYWORK.<br />A LOT MORE GETTING THERE.</span>
              <span className="art-cross cross-one">+</span><span className="art-cross cross-two">+</span>
            </div>
          </div>
          <div className="hero-bottom"><span>01 — STAY IN THE LOOP</span><span>02 — PASS THE BATON</span><span className="scroll-note">SCROLL A LITTLE <span>↓</span></span></div>
        </section>

        <section className="intro-band">
          <div className="wrap intro-inner"><p className="eyebrow">THE SHORT VERSION</p><p className="intro-statement">Not another chat box. <span>A place where agents can actually do the work</span> — with tools, context, and a human keeping an eye on things.</p></div>
        </section>

        <section className="products wrap" id="products">
          <div className="section-top"><p className="eyebrow">PICK YOUR PACE</p><span className="section-count">TWO MODES / ONE CORE</span></div>
          <article className="product-row product-code">
            <div className="product-number">01</div>
            <div className="product-title"><h2>Code<span>.</span></h2><p>Stay close to the work.</p></div>
            <div className="product-description"><p>Your project-aware coding agent. It reads the code, edits files, runs commands, and shows you exactly what changed. No mystery meat commits.</p><a href={`${base}docs/cli`}>Meet Code <span>→</span></a></div>
            <div className="product-aside"><div className="code-glyph"><span>ƒ</span><i>inspect</i><b>(project)</b><em>→</em><strong>ship</strong></div><span className="aside-caption">READ · CHANGE · VERIFY</span></div>
          </article>
          <article className="product-row product-teammates">
            <div className="product-number">02</div>
            <div className="product-title"><h2>Teammates<span>.</span></h2><p>Give the task some legs.</p></div>
            <div className="product-description"><p>Persistent agents with their own memory, skills, conversations, and schedules. Hand off the ongoing stuff. Come back to actual progress.</p><a href={`${base}docs/teammates`}>Meet Teammates <span>→</span></a></div>
            <div className="product-aside teammate-aside"><div className="orbit orbit-a"></div><div className="orbit orbit-b"></div><span className="orbit-core">T</span><span className="orbit-label">REMEMBERS<br />WHAT MATTERS</span></div>
          </article>
        </section>

        <section className="loop-section" id="loop">
          <div className="wrap loop-inner">
            <div className="loop-heading"><p className="eyebrow">NO MAGIC TRICKS</p><h2>Good work is<br /><span>a repeatable thing.</span></h2><p>Luma gives the model a useful set of tools, runs what it asks for, and brings back the result. Then it decides what to do next.</p></div>
            <div className="loop-diagram">
              <div className="loop-line"></div>
              <div className="loop-step"><b>01</b><span className="loop-dot dot-blue"></span><strong>Understand</strong><small>Read the project</small></div>
              <div className="loop-step"><b>02</b><span className="loop-dot dot-orange"></span><strong>Act</strong><small>Use a real tool</small></div>
              <div className="loop-step"><b>03</b><span className="loop-dot dot-blue"></span><strong>Check</strong><small>Inspect the result</small></div>
              <div className="loop-step"><b>04</b><span className="loop-dot dot-orange"></span><strong>Continue</strong><small>Or ask for approval</small></div>
              <p className="loop-footnote">THE AGENT LOOP · WITH RECEIPTS</p>
            </div>
          </div>
        </section>

        <section className="principles wrap">
          <div className="principles-lead"><p className="eyebrow">A FEW NON-NEGOTIABLES</p><h2>Useful by design.<br /><span>Not clever for show.</span></h2></div>
          <div className="principle-list">
            <article><span>01</span><div><h3>Your model. Your call.</h3><p>Connect supported hosted providers or a local model endpoint. Keep control of the setup.</p></div></article>
            <article><span>02</span><div><h3>Show your work.</h3><p>Tool activity and proposed changes stay visible. Sensitive operations can require approval.</p></div></article>
            <article><span>03</span><div><h3>Context that sticks.</h3><p>Skills, project context, teammate memory, and task history can outlast one conversation.</p></div></article>
            <article><span>04</span><div><h3>One core. Room to grow.</h3><p>A shared Rust runtime supports the terminal, desktop workspace, and local dashboard.</p></div></article>
          </div>
        </section>

        <section className="closing">
          <div className="wrap closing-inner">
            <div className="closing-copy"><p className="eyebrow">OPEN SOURCE · BUILT IN RUST</p><h2>Go on, make<br />something <span>move.</span></h2><p>Start with Code. Keep a Teammate around for the rest.</p><div className="closing-actions"><a className="button button-paper" href={`${repo}/releases`}>Get the latest <Arrow /></a><a className="closing-docs" href={`${base}docs/`}>Or read the docs first →</a></div></div>
            <img className="closing-mark" src={`${base}luma-mark.svg`} alt="" />
            <span className="closing-annotation">LESS BUSYWORK<br />MORE GOOD WORK</span>
          </div>
        </section>
      </main>

      <footer className="footer wrap"><Brand /><span>Made for the work between the prompts.</span><div><a href={`${base}docs/`}>Docs</a><a href={`${repo}/issues`}>Issues</a><a href={`${repo}/releases`}>Releases</a><a href={repo}>GitHub <Arrow /></a></div></footer>
    </div>
  )
}

export default App
