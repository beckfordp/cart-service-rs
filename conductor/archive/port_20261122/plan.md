# Plan — port cart-service (gluon all-Rust Stage A, GREEN)

Spec: [spec.md](./spec.md).

## Phase 1: Port

- [x] Task 1.1: domain (ids, sku, quantity, cart) — tests first `2b11b4b`
- [x] Task 1.2: Redis store with the Scala scripts — tests first (throwaway Redis) `2b11b4b`
- [x] Task 1.3: routes and errors — tests first (in-memory store) `2b11b4b`
- [x] Task 1.4: docs (OpenAPI of the five operations, empty AsyncAPI) `2b11b4b`
- [x] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md): gluon `bin/verify-port cart-service` all MET (F, D, harness, ci; evidence `docs/ports/evidence/cart-service/green.txt`); CI 96.03% lines

## Archive (2026-11-22)

Reviewed: no High/Medium. Needed gluon-rs `Cache::query` (e46e911). Deliberate
differences: Redis down fails fast (A7); AsyncAPI served (A6); the preflight lists
service-basics' methods, not Scala's GET/POST/DELETE (A8). The shared CI gate is 80%;
Stage A's 90% is read from the TOTAL line.
