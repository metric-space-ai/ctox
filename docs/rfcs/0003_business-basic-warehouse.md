# RFC 0003: Business Basic Warehouse

Status: Draft - source-audited research baseline, not yet implementation-approved
Created: 2026-05-07
Audience: CTOX core, Business Basic template maintainers, implementation agents

## 1. Decision Summary

This RFC is not ready to jump straight into implementation from the old spec shape. The warehouse scope is implementable for M0/M1, but only after the design is constrained by real inventory implementations and the Business Basic runtime patterns.

The source audit below changes the implementation baseline:

- Use the existing shared `business_outbox_events` pattern. Do not create a separate `warehouse_outbox_events` table unless a separate dispatcher is explicitly designed.
- Include `inventory_owner_party_id` from the first inventory migration. 3PL/customer-owned stock cannot be cleanly added later if movements, balances, reservations, and picking documents are owner-blind.
- Use a non-null deterministic `balance_key` for `stock_balances`. Do not rely on a PostgreSQL unique constraint over nullable `lot_id` / `serial_id`.
- Treat reservations as first-class workflow documents, not as direct edits to available stock. Reserved quantity must only move through reservation commands.
- Split the scope into M0/M1/M2/M3. M0/M1 are implementable after this RFC is accepted; M2/M3 need product decisions, not more generic warehouse research.

## 2. Scope

M0 establishes the inventory kernel:

- Inventory item master
- Warehouses and internal locations
- Stock movement ledger
- Stock balances
- Inventory owner dimension
- Shared outbox events
- Command idempotency and audit trail
- Replay and consistency checks

M1 adds commercial availability:

- Reservations
- Allocation/release/cancellation/sale lifecycle
- Pick list skeleton
- Receiving and putaway skeleton
- Basic shipment/fulfillment documents
- Return intake skeleton

M2 adds warehouse operations:

- Scanner sessions
- Pick/pack/ship execution
- Putaway execution
- Cycle counts and stock adjustments
- Labels/tracking carrier metadata

M3 remains extension territory:

- WES/MFC/robotics
- Slotting optimization
- Multi-node orchestration
- Offline-first mobile sync
- Complex wave planning
- Advanced 3PL billing

## 3. Source Audit

The following implementation sources were inspected as code, not just product docs.

Source repositories:

- Vendure: https://github.com/vendure-ecommerce/vendure
- Medusa: https://github.com/medusajs/medusa
- Spree: https://github.com/spree/spree
- ERPNext: https://github.com/frappe/erpnext
- OpenBoxes: https://github.com/openboxes/openboxes
- CTOX Business Basic template: local repository paths under `templates/business-basic`

| System | Source evidence | Design consequence |
| --- | --- | --- |
| Business Basic template | `templates/business-basic/packages/db/src/schema.ts`, `templates/business-basic/packages/db/drizzle/0007_business_accounting_engine.sql`, `templates/business-basic/packages/accounting/src/workflow/outbox.ts`, `templates/business-basic/packages/accounting/src/workflow/commands.ts` | Warehouse must align with `company_id`, idempotent commands, audit events, and shared `business_outbox_events`. |
| Vendure | `vendure/packages/core/src/entity/stock-level/stock-level.entity.ts`, `stock-movement.entity.ts`, `allocation.entity.ts`, `sale.entity.ts`, `release.entity.ts`, `cancellation.entity.ts`, `stock-location-strategy.ts`, `multi-channel-stock-location-strategy.ts`, `stock-movement.service.ts` | Separate on-hand and allocated quantities. Model allocation/release/sale/cancellation as explicit movement types. Put location/channel strategy behind a service boundary. |
| Medusa | `medusa/packages/modules/inventory/src/models/inventory-level.ts`, `reservation-item.ts`, `inventory-module.ts`, `core-flows/src/reservation/steps/create-reservations.ts`, `fulfillment/src/models/fulfillment*.ts`, `stock-location/src/models/stock-location.ts` | Reservation items adjust reserved quantity through a service transaction and lock scope. Fulfillment needs location, provider, shipping option, labels, item rows, and metadata. |
| Spree | `spree/core/app/models/spree/stock_item.rb`, `stock_movement.rb`, `stock_location.rb`, `order_inventory.rb`, `shipment.rb`, `inventory_unit.rb`, `return_authorization.rb`, `return_item.rb`, `customer_return.rb`, `stock/packer.rb`, `stock/package.rb` | Keep stock item unique per variant/location; movements are readonly after creation; order inventory must handle backorder/on-hand units; shipment cancellation/restock and returns are separate workflows. |
| ERPNext | `erpnext/stock/doctype/stock_reservation_entry/stock_reservation_entry.py`, `erpnext/stock/stock_ledger.py`, `erpnext/stock/doctype/pick_list/pick_list.py`, `erpnext/stock/doctype/purchase_receipt/purchase_receipt.py` | Reservation has voucher links, partial-reservation policy, serial/batch validation, and Bin reserved-stock updates. Ledger replay must order by posting datetime and lock future entries. Pick lists validate batch/serial/warehouse availability. Receipts can trigger putaway and reservation. |
| OpenBoxes | `openboxes/grails-app/domain/org/pih/warehouse/inventory/InventoryItem.groovy`, `InventoryLevel.groovy`, `Shipment.groovy`, `ShipmentItem.groovy`, `services/.../PicklistService.groovy`, `PutawayService.groovy`, `FulfillmentService.groovy`, `CycleCountService.groovy` | Warehouse operations need bin/location, lot/expiry/status, picklist ATP refresh, explicit shipment items, containers, putaway linkage, recounts, custom count rows, and final adjustment transactions. |

Rejected as insufficient for this baseline:

- Fleetbase repository clone did not expose enough relevant inventory/warehouse implementation code for this RFC.
- OCA `wms` clone was mostly repository metadata in the sparse audit. Do not use it as a primary source unless a concrete OCA addon is selected and inspected.

## 4. Core Invariants

### 4.1 Balance Identity

`stock_balances` must have a deterministic non-null `balance_key`.

The key must include:

- `company_id`
- `inventory_owner_party_id`, with a system-owned sentinel when there is no separate owner
- `inventory_item_id`
- `warehouse_location_id`
- `stock_status`
- `lot_id`, with a sentinel when not lot-tracked
- `serial_id`, with a sentinel when not serial-tracked
- optional future dimensions only if they are part of availability semantics

Required database rule:

```sql
unique (company_id, balance_key)
```

Do not implement this as only:

```sql
unique (company_id, inventory_item_id, warehouse_location_id, stock_status, lot_id, serial_id)
```

PostgreSQL allows multiple `NULL` values in unique constraints, so nullable lot/serial dimensions would allow duplicate balances for non-tracked items.

### 4.2 Movement Ledger

`stock_movements` is append-only after posting.

Required fields:

- `external_id`
- `company_id`
- `inventory_owner_party_id`
- `inventory_item_id`
- `warehouse_location_id`
- `movement_type`
- `stock_status_from`
- `stock_status_to`
- `quantity`
- `uom`
- `lot_id`
- `serial_id`
- `source_type`
- `source_id`
- `source_line_id`
- `idempotency_key`
- `posted_at`
- `created_at`
- `created_by`

Rules:

- Every posted movement updates exactly one or two balances.
- Movement replay must be deterministic by `(posted_at, created_at, id)`.
- Corrections are new movements, never in-place edits.
- Negative stock is disallowed in M0/M1 unless a warehouse policy explicitly allows it for the item and owner.
- Serial-tracked items require unit quantity per serial movement unless the item policy explicitly allows serialized bundle handling.

### 4.3 Owner Dimension

`inventory_owner_party_id` is required on:

- `stock_movements`
- `stock_balances`
- `stock_reservations`
- `stock_reservation_lines`
- `pick_lists`
- `pick_list_lines`
- `shipments`
- `shipment_lines`
- `inventory_adjustments`

Locations may optionally declare a default owner, but location ownership must not replace balance ownership. A 3PL warehouse can hold inventory from multiple owners in the same physical location.

### 4.4 Reservations

Reservations are workflow records, not raw balance edits.

Minimum reservation model:

- `stock_reservations`
- `stock_reservation_lines`
- status: `draft`, `reserved`, `partially_reserved`, `released`, `partially_consumed`, `consumed`, `cancelled`, `expired`
- source fields: `source_type`, `source_id`, `source_line_id`
- `allow_partial_reservation`
- `allow_backorder`
- optional serial/batch selection rows

Rules:

- Reserved quantity changes only through reservation commands.
- Reservation commands must lock by inventory item and affected balance keys.
- Reservation availability is `on_hand - reserved - picked - packed`, adjusted by policy.
- Reservation release, sale, cancellation, and shipment consumption are separate transitions.
- Serial/batch reservations must validate that selected serials/batches are not reserved elsewhere.

### 4.5 Availability

Availability service signature:

```ts
getAvailability({
  companyId,
  inventoryOwnerPartyId,
  inventoryItemId,
  locationScope,
  channelId,
  stockStatus,
  lotId,
  serialId,
  requiredBy,
  includeBackorderPolicy
})
```

Availability must be a service computation. The UI and API must not manually recompute availability from arbitrary tables.

Vendure and Medusa both show that commercial availability is not just physical on-hand. Channel/location strategy, allocated/reserved quantities, thresholds, and backorder policy belong behind a service boundary.

## 5. Data Model Baseline

### 5.1 Tables

M0 tables:

- `inventory_items`
- `warehouse_locations`
- `warehouse_policies`
- `stock_movements`
- `stock_balances`
- `inventory_command_log`
- `inventory_audit_events`

M1 tables:

- `stock_reservations`
- `stock_reservation_lines`
- `pick_lists`
- `pick_list_lines`
- `receipts`
- `receipt_lines`
- `putaway_tasks`
- `shipments`
- `shipment_lines`
- `return_authorizations`
- `return_lines`

M2 tables:

- `scanner_sessions`
- `scan_events`
- `cycle_counts`
- `cycle_count_lines`
- `inventory_adjustments`
- `fulfillment_labels`
- `shipment_packages`
- `shipment_tracking_events`

Do not introduce:

- `warehouse_items`
- `warehouse_stock_items`
- `warehouse_outbox_events`

The canonical namespace is `inventory_*`, `stock_*`, `warehouse_*`, `pick_*`, `receipt_*`, `shipment_*`, `return_*`.

### 5.2 Required M0 Uniqueness

```sql
unique (external_id)
unique (company_id, balance_key)
unique (company_id, idempotency_key)
```

`inventory_items` may expose SKU uniqueness as:

```sql
unique (company_id, sku)
```

only if SKU is tenant-local. If SKU is owner-specific, use:

```sql
unique (company_id, inventory_owner_party_id, sku)
```

### 5.3 Versioning

Mutable workflow tables need optimistic concurrency:

- `stock_reservations.version`
- `pick_lists.version`
- `receipts.version`
- `putaway_tasks.version`
- `shipments.version`
- `return_authorizations.version`
- `cycle_counts.version`

Append-only ledgers do not need mutable versioning, but command idempotency must be enforced.

## 6. Workflow Baseline

### 6.1 Receipt And Putaway

Receipt flow:

1. Create receipt from purchase order, ASN, manual receipt, or return intake.
2. Validate item, owner, lot/serial policy, expiry, quality status, and destination warehouse.
3. Post inbound movement into receiving status.
4. Create putaway tasks with explicit receipt-line linkage.
5. Complete putaway by moving quantity from receiving to available/quarantine/damaged.

OpenBoxes shows that weak receipt-to-putaway linkage creates heuristic matching later. CTOX should model the relation directly in M1.

### 6.2 Reservation And Allocation

Reservation flow:

1. Source document requests stock.
2. Availability service computes candidates.
3. Reservation command locks affected balance keys.
4. Reservation lines record selected owner/location/lot/serial dimensions.
5. Balance reserved quantity is updated through the reservation service.
6. Outbox event is emitted to `business_outbox_events`.

Allocation/pick flow:

1. Pick list is generated from reservation or order source.
2. Candidate lines are sorted by strategy: FEFO/FIFO, location priority, channel priority, or manual override.
3. Pick list validates available-to-pick at bin/lot/serial level.
4. Picked quantity transitions to picked status.
5. Cancellation releases unpicked/picked stock according to state.

### 6.3 Fulfillment And Shipment

Shipment flow:

1. Create shipment with origin location, owner, provider, shipping option, and address.
2. Add shipment lines from picked quantities.
3. Pack into packages/containers.
4. Generate labels/tracking metadata if provider integration exists.
5. Ship by posting stock movement from picked/packed to shipped.
6. Cancellation/restock creates compensating movements.

Medusa and Spree both show fulfillment as its own document family rather than a thin flag on an order.

### 6.4 Returns

Return flow:

1. Create return authorization against shipped inventory.
2. Receive return line.
3. Decide acceptance and resellable status.
4. If resellable, post movement into available or inspection status.
5. If rejected/damaged, post into quarantine/damaged.
6. Reimbursement/accounting remains outside the inventory kernel but must be linked.

Spree separates return authorization, customer return, return item, acceptance status, reception status, and restock decision. CTOX should keep that separation in M1/M2.

### 6.5 Cycle Count

Cycle count flow:

1. Create count request by item/location/owner scope.
2. Snapshot current availability into count lines.
3. Allow custom count rows for newly found lot/bin combinations.
4. Submit count.
5. If variance requires recount, create recount lines with a new count index.
6. Close count by posting adjustment movements.

OpenBoxes shows that recounts may need fresh availability plus custom rows from the prior count. CTOX should not model cycle count as a single flat adjustment form.

## 7. Integration With Business Basic

Warehouse must follow the existing Business Basic runtime style:

- Commands carry `companyId`, `type`, `refType`, `refId`, `requestedBy`, `requestedAt`, `idempotencyKey`.
- Shared outbox rows go into `business_outbox_events`.
- Topics should use `warehouse.*`, for example `warehouse.stock_reserved`, `warehouse.stock_moved`, `warehouse.shipment_shipped`.
- Audit rows may start as `inventory_audit_events`, but if Business Basic later generalizes accounting audit into shared business audit, warehouse should migrate to the shared audit table.
- Database definitions should live in `templates/business-basic/packages/db/src/schema.ts`.
- Runtime code should live in a new `templates/business-basic/packages/warehouse`.

The first implementation must include smoke tests equivalent to the accounting smoke:

- command idempotency
- balance uniqueness
- replay consistency
- reserve/release/consume lifecycle
- owner separation
- serial/batch duplicate prevention

## 8. Implementation Slices

### M0 PR

Deliver:

- schema for `inventory_items`, `warehouse_locations`, `warehouse_policies`, `stock_movements`, `stock_balances`, `inventory_command_log`
- deterministic `balance_key`
- command helper and idempotency
- movement posting service
- replay check
- shared outbox event emission
- tests for balance uniqueness, owner separation, movement replay, and duplicate idempotency keys

Hard gate:

- No nullable-dimension uniqueness bug.
- No separate warehouse outbox.
- No owner-blind stock balances.

### M1 PR

Deliver:

- reservation aggregate
- availability service
- pick list skeleton
- receipt and putaway skeleton
- shipment skeleton
- return authorization skeleton
- tests for reserve, partial reserve, release, consume, owner separation, lot/serial conflict, and pick candidate validation

Hard gate:

- Reserved quantity can only change through reservation commands.
- Serial/batch reservations cannot double-book.
- Pick list cannot over-pick.

### M2 PR

Deliver:

- scanner sessions and scan event ingestion
- cycle count and adjustment workflow
- fulfillment labels and packages
- shipment tracking events
- richer putaway/pick execution

Hard gate:

- Scan ingestion must be idempotent.
- Cycle count closure must post movements, not mutate balances directly.
- Shipment cancellation/restock must be compensating movements.

### M3 PR

Deliver only after product decision:

- WES/MFC adapters
- robotics events
- multi-node orchestration
- advanced wave planning
- 3PL billing
- offline sync

Hard gate:

- Requires a selected real integration target and separate RFC.

## 9. Open Product Decisions

These do not block M0/M1 implementation:

- Which UI workflows ship first: ecommerce order fulfillment, B2B warehouse operations, or internal inventory control?
- Whether `inventory_owner_party_id` is visible in the default UI or hidden behind a "customer-owned stock" feature flag.
- Whether SKU uniqueness is tenant-level or owner-level.
- Whether negative stock is completely disabled in v1 or allowed by explicit item policy.
- Whether accounting COGS integration belongs in RFC 0003 or a separate accounting-inventory RFC.

These block M2/M3:

- Offline scanner architecture.
- Carrier/provider integration target.
- WES/MFC vendor target.
- Wave planning complexity.
- 3PL billing model.

## 10. Research Verdict

The warehouse spec is now research-grounded enough for M0/M1 implementation planning.

It is not ready for direct all-scope implementation. The correct next step is:

1. Accept this RFC as the M0/M1 source baseline.
2. Create an implementation branch for M0 only.
3. Implement M0 with tests and replay checks.
4. Revisit M1 after M0 schema and runtime conventions are proven in the template.

No further broad research is needed before M0/M1. More research should be targeted only when entering M2/M3 or when choosing a concrete external carrier/scanner/WES integration.

## 11. Warehouse User Stories

Die produktorientierten User Stories fuer das virtuelle Lager sind in
[`templates/business-basic/docs/warehouse-user-stories.md`](../templates/business-basic/docs/warehouse-user-stories.md)
ausgelagert.

Der Story-Satz enthaelt WH-001 bis WH-050. Jede Story beschreibt die manuelle optimale UI und die KI-gestuetzte Bedienung ueber CTOX Chat/Rechtsklick.
