# Spec — port cart-service (gluon all-Rust Stage A, GREEN)

## Overview
The Scala cart-service in idiomatic Rust on the template's shape: carts in Redis as the
Scala service stores them (hash `cart:<id>`, Lua scripts, 24 h TTL refreshed per
write), `POST /carts`, `GET`/`DELETE /carts/{id}`, `POST /carts/{id}/items`,
`DELETE /carts/{id}/items/{sku}`, health, readiness on Redis, and the docs.

## Functional requirements
CART-F1–F10, CART-D1–D2 (tests/contract `docs/behaviours.md`). Redis down fails fast
(503 readiness, 500 reads) where Scala hangs (decision A7).

## Acceptance criteria
`scripts/ci.sh` green, ≥90% line coverage; `bin/verify-port cart-service` all MET.

## Out of scope
Wire rows: metrics (`cart_*` journey counters, `db.client.operation.duration`), log
messages, field order, the CORS method list (decision A8).
