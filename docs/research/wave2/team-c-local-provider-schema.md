# Team C — Local Provider Schema Draft (C2)

## Proposed Config Additions

### at-intelligence profile
- `provider_kind = \"Local\"`
- `base_url = \"http://127.0.0.1:8000\"`
- `model = \"qwen2.5-coder-7b-instruct\"`
- `timeout_ms = 120000`
- `healthcheck_path = \"/v1/models\"`

### routing metadata
- `class = \"small\" | \"medium\" | \"large\"`
- `max_context_tokens`
- `supports_tools`
- `supports_json_mode`

## Failover Chain
1. `Anthropic` primary
2. `Local` secondary (vllm.rs OpenAI-compatible endpoint)
3. `OpenRouter` tertiary

## Trigger Conditions
- 429 / rate-limited
- network timeout
- provider circuit breaker open

## Observability Fields
- `provider_name`
- `selected_model`
- `fallback_from`
- `fallback_reason`
- `latency_ms`
- `input_tokens`, `output_tokens`, `cost_estimate`
