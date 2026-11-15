# Configuration and models

Luma connects to OpenAI-compatible chat-completion endpoints. Supported local
and hosted options include Ollama, LM Studio, llama.cpp servers, and other
compatible providers.

An Ollama endpoint commonly uses:

```text
http://localhost:11434/v1/chat/completions
```

For example, the Rust model client can be configured as:

```rust
let model = OpenAICompatibleModel::new(
    "http://localhost:11434/v1/chat/completions",
    "qwen2.5-coder:3b",
);
```

Run `luma setup` to configure a provider and model. Provider configuration is
stored separately from the agent runtime so the model endpoint can be changed
without changing the agent architecture.

Luma can run with smaller coding models, although model capability still has a
substantial effect on task quality. Tool access grounds model decisions in
actual workspace observations; it does not make all models equally capable.
