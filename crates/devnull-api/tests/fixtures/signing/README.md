# Signing test vectors

Produced in M0.2 (see `docs/plan.md`, D9) with a throwaway key generated for the purpose and never enrolled
anywhere:

- `device_ed25519`, `device_ed25519.pub`: the test key. `enc_ed25519` is the same key encrypted with the passphrase
  `correct horse`.
- `msg-get`, `msg-post`, `msg-upload`: canonical request messages, exactly as the Go client joins them.
- `*.go.b64`: the `X-SSH-Signature` values the original Go client's unmodified `key.go` produced for them (namespace
  `devnull-api`, and `devnull-api-images` for the upload).
