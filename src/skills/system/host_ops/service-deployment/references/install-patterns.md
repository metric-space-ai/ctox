# Install Patterns

Typical local-install pattern:

1. preflight
2. choose package/runtime path
3. install
4. generate admin credentials if needed
5. configure trusted URL or local binding
6. start
7. verify runtime layers in order:
   - process or unit up
   - listener open
   - HTTP/UI up
   - authenticated admin/API access works
   - safe mutating smoke check works when applicable
8. persist secret reference and operator handoff
9. if runtime is up but authenticated or mutating verification fails, mark `needs_repair` instead of `executed`

Typical external-integration pattern:

1. preflight
2. inspect existing integration script or config
3. classify remote endpoint and credentials as truly external
4. ask the owner only for the exact missing values
5. remain `blocked` until they exist

Typical repair pattern after partial success:

1. identify the highest failed verification layer
2. decide whether the cause is:
   - bad secret
   - unknown auth path
   - incomplete bootstrap
   - wrong binding or trusted URL
   - missing migration or initialization step
3. repair that specific layer
4. rerun acceptance verification
