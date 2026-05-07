"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import type { StockBalance, StockStatus, WarehouseLocation, WarehouseState } from "@ctox-business/warehouse";
import type { WarehouseLayoutAction, WarehouseMutationAction } from "../lib/warehouse-runtime";

type WarehouseStorageWorkbenchProps = {
  initialSelectedWarehouseId?: string;
  initialSnapshot: WarehouseState;
  locale: "de" | "en";
  query: {
    locale?: string;
    theme?: string;
    warehouseSearch?: string;
  };
  submoduleId: string;
};

const activeStatuses: StockStatus[] = ["available", "reserved", "picked", "receiving", "in_transit", "packed", "quarantine", "damaged"];

export function WarehouseStorageWorkbench({
  initialSelectedWarehouseId,
  initialSnapshot,
  locale,
  query,
  submoduleId
}: WarehouseStorageWorkbenchProps) {
  const de = locale === "de";
  const router = useRouter();
  const [snapshot, setSnapshot] = useState(initialSnapshot);
  const [selectedWarehouseId, setSelectedWarehouseId] = useState(initialSelectedWarehouseId);
  const [selectedSlotId, setSelectedSlotId] = useState<string | undefined>();
  const [bottomOpen, setBottomOpen] = useState(false);
  const [mode, setMode] = useState<"slot" | "stock" | "count">("slot");
  const [search, setSearch] = useState(query.warehouseSearch ?? "");
  const [dragBalanceKey, setDragBalanceKey] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState(de ? "Bereit" : "Ready");

  const warehouses = snapshot.locations.filter((location) => location.kind === "warehouse");
  const selectedWarehouse = warehouses.find((location) => location.id === selectedWarehouseId) ?? warehouses[0];
  const childLocations = (parentId: string) => snapshot.locations.filter((location) => location.parentId === parentId);
  const descendantIds = (parentId: string): string[] => childLocations(parentId).flatMap((location) => [location.id, ...descendantIds(location.id)]);
  const selectedLocationIds = new Set(selectedWarehouse ? [selectedWarehouse.id, ...descendantIds(selectedWarehouse.id)] : []);
  const selectedSlot = selectedSlotId ? snapshot.locations.find((location) => location.id === selectedSlotId) : undefined;
  const selectedSlotBalances = selectedSlot ? snapshot.balances.filter((balance) => balance.locationId === selectedSlot.id && balance.quantity > 0) : [];
  const visibleWarehouses = warehouses.filter((warehouse) => {
    const needle = search.trim().toLowerCase();
    if (!needle) return true;
    const locationNames = [warehouse.name, ...descendantIds(warehouse.id).map((id) => locationName(id))].join(" ").toLowerCase();
    return locationNames.includes(needle);
  });

  const zones = selectedWarehouse
    ? childLocations(selectedWarehouse.id)
        .filter((location) => location.kind === "zone")
        .map((zone) => ({
          slots: childLocations(zone.id).filter((location) => location.kind === "bin"),
          zone
        }))
    : [];

  const kpis = useMemo(() => {
    const qty = (status: StockStatus) => snapshot.balances
      .filter((balance) => selectedLocationIds.has(balance.locationId) && balance.stockStatus === status)
      .reduce((sum, balance) => sum + balance.quantity, 0);
    return [
      [de ? "Verfuegbar" : "Available", qty("available")],
      [de ? "Reserviert" : "Reserved", qty("reserved")],
      [de ? "Gepickt" : "Picked", qty("picked")],
      [de ? "Sperrbestand" : "Blocked", qty("quarantine") + qty("damaged")]
    ] satisfies Array<[string, number]>;
  }, [de, selectedLocationIds, snapshot.balances]);

  const stockRows = snapshot.balances
    .filter((balance) => selectedLocationIds.has(balance.locationId) && balance.quantity > 0 && activeStatuses.includes(balance.stockStatus))
    .sort((a, b) => b.quantity - a.quantity)
    .slice(0, 18);
  const inboundRows = [
    ...snapshot.receipts.slice(0, 5).map((receipt) => ({
      href: panelHref("receipt", receipt.id, "left-bottom"),
      id: receipt.id,
      meta: `${receipt.status} · ${receipt.lines.length} ${de ? "Positionen" : "lines"}`,
      recordType: "warehouse_receipt",
      title: receipt.sourceId
    })),
    ...snapshot.putawayTasks.slice(0, 5).map((task) => ({
      href: panelHref("location", task.toLocationId, "bottom"),
      id: task.id,
      meta: `${itemName(task.inventoryItemId)} · ${task.quantity} ${de ? "nach" : "to"} ${locationName(task.toLocationId)}`,
      recordType: "warehouse_putaway",
      title: de ? "Einlagerung" : "Putaway"
    }))
  ];
  const outboundRows = snapshot.reservations.slice(0, 8).map((reservation, index) => {
    const quantity = reservation.lines.reduce((sum, line) => sum + line.quantity, 0);
    const picked = reservation.lines.reduce((sum, line) => sum + line.pickedQuantity, 0);
    const shipped = reservation.lines.reduce((sum, line) => sum + line.shippedQuantity, 0);
    return {
      id: reservation.id,
      index,
      picked,
      quantity,
      reservation,
      shipped
    };
  });

  function itemName(id: string) {
    return snapshot.items.find((item) => item.id === id)?.name ?? id;
  }

  function locationName(id: string) {
    return snapshot.locations.find((location) => location.id === id)?.name ?? id;
  }

  function slotQuantity(slotId: string, statuses: StockStatus[] = activeStatuses) {
    return snapshot.balances
      .filter((balance) => balance.locationId === slotId && balance.quantity > 0 && statuses.includes(balance.stockStatus))
      .reduce((sum, balance) => sum + balance.quantity, 0);
  }

  function slotState(slot: WarehouseLocation) {
    const damaged = slotQuantity(slot.id, ["quarantine", "damaged"]);
    const committed = slotQuantity(slot.id, ["reserved", "picked", "packed"]);
    const receiving = slotQuantity(slot.id, ["receiving", "in_transit"]);
    const available = slotQuantity(slot.id, ["available"]);
    if (!slot.pickable || damaged > 0) return "blocked";
    if (committed > 0) return "committed";
    if (receiving > 0) return "receiving";
    if (available > 0) return "stocked";
    return "empty";
  }

  function panelHref(panel: string, recordId: string, drawer: "bottom" | "left-bottom" | "right") {
    const params = new URLSearchParams();
    if (query.locale) params.set("locale", query.locale);
    if (query.theme) params.set("theme", query.theme);
    if (selectedWarehouse?.id) params.set("selectedId", selectedWarehouse.id);
    if (search) params.set("warehouseSearch", search);
    params.set("panel", panel);
    params.set("recordId", recordId);
    params.set("drawer", drawer);
    return `/app/business/${submoduleId}?${params.toString()}`;
  }

  function fulfillmentHref(recordId?: string) {
    const params = new URLSearchParams();
    if (query.locale) params.set("locale", query.locale);
    if (query.theme) params.set("theme", query.theme);
    if (selectedWarehouse?.id) params.set("selectedId", selectedWarehouse.id);
    if (recordId) params.set("recordId", recordId);
    return `/app/business/fulfillment?${params.toString()}`;
  }

  async function runLayout(layoutAction: WarehouseLayoutAction, input: Record<string, unknown> = {}, success?: string) {
    setBusy(layoutAction);
    try {
      const response = await fetch("/api/business/warehouse", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ layoutAction, ...input })
      });
      const payload = await response.json() as { error?: string; ok?: boolean; snapshot?: WarehouseState };
      if (!response.ok || payload.ok === false || !payload.snapshot) throw new Error(payload.error ?? "Warehouse action failed");
      setSnapshot(payload.snapshot);
      setMessage(success ?? (de ? "Gespeichert" : "Saved"));
      router.refresh();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Warehouse action failed");
    } finally {
      setBusy(null);
    }
  }

  async function runReservation(action: WarehouseMutationAction, reservationId: string) {
    setBusy(`${action}:${reservationId}`);
    try {
      const response = await fetch("/api/business/warehouse", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ reservationId, warehouseAction: action })
      });
      const payload = await response.json() as { error?: string; ok?: boolean; snapshot?: WarehouseState };
      if (!response.ok || payload.ok === false || !payload.snapshot) throw new Error(payload.error ?? "Warehouse action failed");
      setSnapshot(payload.snapshot);
      setMessage(de ? "Auftrag aktualisiert" : "Order updated");
      router.refresh();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Warehouse action failed");
    } finally {
      setBusy(null);
    }
  }

  function selectSlot(slotId: string) {
    setSelectedSlotId(slotId);
    setBottomOpen(true);
  }

  function selectedBalanceLabel(balance: StockBalance) {
    return `${itemName(balance.inventoryItemId)} · ${balance.quantity} · ${balance.stockStatus}`;
  }

  return (
    <div className={`warehouse-workbench warehouse-storage-workbench warehouse-ops-workbench ${bottomOpen ? "has-bottom-module" : ""}`} data-context-module="business" data-context-submodule={submoduleId}>
      <section className="warehouse-panel warehouse-left warehouse-inbound-rail" aria-label={de ? "Wareneingang und Einlagerung" : "Receiving and putaway"}>
        <header className="warehouse-panel-head">
          <div>
            <h2>{de ? "Wareneingang" : "Inbound"}</h2>
            <p>{de ? "Annahme, Einlagerung, Inventur" : "Receiving, putaway, count"}</p>
          </div>
          <a className="warehouse-subtle-action" href={panelHref("warehouse-admin", selectedWarehouse?.id ?? "warehouse", "left-bottom")}>{de ? "Pflegen" : "Manage"}</a>
        </header>
        <div className="warehouse-tool-row">
          <input className="warehouse-search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={de ? "Lager, Zone oder Slot suchen" : "Find warehouse, zone or slot"} />
          <button className="warehouse-subtle-action" disabled={busy !== null} onClick={() => void runLayout("createWarehouse", {}, de ? "Lager angelegt" : "Warehouse created")} type="button">
            {de ? "Neu" : "New"}
          </button>
        </div>
        <div className="warehouse-source-list">
          {visibleWarehouses.map((warehouse) => {
            const ids = new Set([warehouse.id, ...descendantIds(warehouse.id)]);
            const available = snapshot.balances.filter((balance) => ids.has(balance.locationId) && balance.stockStatus === "available").reduce((sum, balance) => sum + balance.quantity, 0);
            const sections = childLocations(warehouse.id).filter((location) => location.kind === "zone").length;
            return (
              <button
                aria-current={warehouse.id === selectedWarehouse?.id ? "page" : undefined}
                className={warehouse.id === selectedWarehouse?.id ? "warehouse-source-card is-active" : "warehouse-source-card"}
                data-context-item
                data-context-label={warehouse.name}
                data-context-module="business"
                data-context-record-id={warehouse.id}
                data-context-record-type="warehouse_source"
                data-context-submodule={submoduleId}
                key={warehouse.id}
                onClick={() => {
                  setSelectedWarehouseId(warehouse.id);
                  setSelectedSlotId(undefined);
                  setBottomOpen(false);
                }}
                type="button"
              >
                <span className="warehouse-avatar">{warehouse.name.slice(0, 1)}</span>
                <span><strong>{warehouse.name}</strong><small>{sections} {de ? "Bereiche" : "areas"}</small></span>
                <em>{available}</em>
              </button>
            );
          })}
        </div>
        <div className="warehouse-left-metrics">
          {kpis.map(([label, value]) => <div key={label}><span>{label}</span><strong>{value}</strong></div>)}
        </div>
        <section className="warehouse-flow-panel">
          <header>
            <strong>{de ? "Einlagerung" : "Putaway"}</strong>
            <button disabled={busy !== null || !selectedWarehouse} onClick={() => selectedWarehouse ? void runLayout("createSection", { parentId: selectedWarehouse.id }, de ? "Bereich angelegt" : "Section added") : undefined} type="button">
              {de ? "Bereich" : "Section"}
            </button>
          </header>
          <div>
            {inboundRows.length > 0 ? inboundRows.map((row) => (
              <a className="warehouse-flow-item" data-context-item data-context-label={row.title} data-context-record-id={row.id} data-context-record-type={row.recordType} href={row.href} key={`${row.recordType}-${row.id}`}>
                <span><strong>{row.title}</strong><small>{row.meta}</small></span>
              </a>
            )) : <span className="warehouse-empty-note">{de ? "Keine offenen Wareneingaenge." : "No inbound work."}</span>}
          </div>
        </section>
      </section>

      <section className="warehouse-panel warehouse-center warehouse-map-stage" aria-label={de ? "Virtuelles Lager" : "Virtual warehouse"}>
        <header className="warehouse-panel-head">
          <div>
            <h2>{de ? "Virtuelles Lager" : "Virtual warehouse"}</h2>
            <p>{selectedWarehouse?.name ?? "-"} · {zones.length} {de ? "Bereiche" : "sections"} · {message}</p>
          </div>
          <div className="warehouse-head-actions">
            <button className="warehouse-subtle-action" disabled={busy !== null || !selectedWarehouse} onClick={() => selectedWarehouse ? void runLayout("duplicateLocation", { parentId: selectedWarehouse.id }, de ? "Lager dupliziert" : "Warehouse duplicated") : undefined} type="button">
              {de ? "Duplizieren" : "Duplicate"}
            </button>
            <a className="warehouse-subtle-action" href={panelHref("business-set", "warehouse-replay", "right")}>{de ? "Audit" : "Audit"}</a>
          </div>
        </header>
        <div className="warehouse-layout-map warehouse-layout-field">
          {zones.length === 0 ? (
            <div className="warehouse-layout-empty">
              <strong>{de ? "Noch keine Bereiche" : "No sections yet"}</strong>
              <button disabled={busy !== null || !selectedWarehouse} onClick={() => selectedWarehouse ? void runLayout("createSection", { parentId: selectedWarehouse.id }) : undefined} type="button">
                {de ? "Ersten Bereich anlegen" : "Add first section"}
              </button>
            </div>
          ) : zones.map(({ zone, slots }) => {
            const used = slots.filter((slot) => slotQuantity(slot.id) > 0).length;
            return (
              <article className="warehouse-zone-card warehouse-zone-lane" data-context-item data-context-label={zone.name} data-context-record-id={zone.id} data-context-record-type="warehouse_zone" key={zone.id}>
                <header>
                  <span><strong>{zone.name}</strong><small>{used}/{slots.length} {de ? "Slots genutzt" : "slots used"}</small></span>
                  <button disabled={busy !== null} onClick={() => void runLayout("createSlot", { parentId: zone.id, slotCount: 4 }, de ? "Slots angelegt" : "Slots added")} type="button">+4</button>
                </header>
                <div className="warehouse-slot-grid warehouse-visual-slot-grid">
                  {slots.length > 0 ? slots.map((slot) => {
                    const state = slotState(slot);
                    const balances = snapshot.balances.filter((balance) => balance.locationId === slot.id && balance.quantity > 0).slice(0, 2);
                    return (
                      <button
                        className={`warehouse-slot warehouse-visual-slot is-${state} ${slot.id === selectedSlotId ? "is-selected" : ""}`}
                        data-context-item
                        data-context-label={slot.name}
                        data-context-module="business"
                        data-context-record-id={slot.id}
                        data-context-record-type="warehouse_slot"
                        data-context-submodule={submoduleId}
                        key={slot.id}
                        onClick={() => selectSlot(slot.id)}
                        onDragOver={(event) => {
                          if (dragBalanceKey) event.preventDefault();
                        }}
                        onDrop={(event) => {
                          event.preventDefault();
                          if (dragBalanceKey && dragBalanceKey !== slot.id) {
                            void runLayout("moveStock", {
                              balanceKey: dragBalanceKey,
                              targetLocationId: slot.id
                            }, de ? "Bestand verschoben" : "Stock moved");
                            setDragBalanceKey(null);
                            selectSlot(slot.id);
                          }
                        }}
                        type="button"
                      >
                        <strong>{slot.name}</strong>
                        <span>{slotQuantity(slot.id) || (de ? "frei" : "free")}</span>
                        {balances.length > 0 ? <i>{balances.map((balance) => (
                          <b
                            draggable
                            key={balance.balanceKey}
                            onDragStart={(event) => {
                              event.stopPropagation();
                              setDragBalanceKey(balance.balanceKey);
                              event.dataTransfer.setData("text/plain", balance.balanceKey);
                            }}
                          >
                            {itemName(balance.inventoryItemId).slice(0, 10)}
                          </b>
                        ))}</i> : null}
                      </button>
                    );
                  }) : <span className="warehouse-slot-placeholder">{de ? "Keine Slots" : "No slots"}</span>}
                </div>
              </article>
            );
          })}
        </div>
      </section>

      <section className="warehouse-panel warehouse-right warehouse-outbound-rail" aria-label={de ? "Warenkorb und Ausgang" : "Cart and outbound"}>
        <header className="warehouse-panel-head">
          <div>
            <h2>{de ? "Warenkorb" : "Cart"}</h2>
            <p>{de ? "Reservieren, picken, packen, uebergeben" : "Reserve, pick, pack, hand off"}</p>
          </div>
          <a className="warehouse-subtle-action" href={fulfillmentHref()}>{de ? "Auftragsmodul" : "Fulfillment"}</a>
        </header>
        <div className="warehouse-position-list">
          <section className="warehouse-match-panel warehouse-cart-panel">
            <h3>{selectedSlot ? selectedSlot.name : de ? "Ausgangsqueue" : "Outbound queue"}</h3>
            <div className="warehouse-source-match-list">
              {(selectedSlotBalances.length > 0 ? selectedSlotBalances : stockRows.slice(0, 8)).map((row) => (
                <a
                  className={`warehouse-source-match is-${row.stockStatus}`}
                  data-context-item
                  data-context-label={selectedBalanceLabel(row)}
                  data-context-record-id={row.balanceKey}
                  data-context-record-type="stock_balance"
                  href={panelHref("balance", row.balanceKey, "right")}
                  key={row.balanceKey}
                >
                  <span><strong>{itemName(row.inventoryItemId)}</strong><small>{locationName(row.locationId)} · {row.stockStatus}</small></span>
                  <em>{row.quantity}</em>
                </a>
              ))}
            </div>
          </section>
          <section className="warehouse-match-panel warehouse-cart-panel">
            <h3>{de ? "Auftragsausgang" : "Order outbound"}</h3>
            <div className="warehouse-source-match-list">
              {outboundRows.map(({ id, picked, quantity, reservation, shipped }) => {
                const canPick = picked < quantity && reservation.status !== "consumed" && reservation.status !== "cancelled";
                const canShip = picked > shipped;
                return (
                  <div className="warehouse-source-match warehouse-cart-order" data-context-item data-context-label={reservation.sourceId} data-context-record-id={id} data-context-record-type="warehouse_reservation" key={id}>
                    <a href={panelHref("reservation", id, "right")}><span><strong>{reservation.sourceId}</strong><small>{picked}/{quantity} {de ? "gepickt" : "picked"} · {reservation.status}</small></span></a>
                    <span className="warehouse-cart-actions">
                      <button disabled={busy !== null || !canPick} onClick={() => void runReservation("pick", id)} type="button">{de ? "Pick" : "Pick"}</button>
                      <button disabled={busy !== null || !canShip} onClick={() => void runReservation("ship", id)} type="button">{de ? "Ship" : "Ship"}</button>
                    </span>
                  </div>
                );
              })}
            </div>
          </section>
        </div>
      </section>

      <aside className={`warehouse-bottom-module ${bottomOpen ? "is-open" : ""}`} aria-label={de ? "Arbeitsmodul" : "Work module"}>
        <button className="warehouse-bottom-tab" onClick={() => setBottomOpen(!bottomOpen)} type="button">
          <span>{selectedSlot ? selectedSlot.name : de ? "Arbeitsmodul" : "Work module"}</span>
          <strong>{bottomOpen ? (de ? "Zuklappen" : "Close") : (de ? "Aufklappen" : "Open")}</strong>
        </button>
        {bottomOpen ? (
          <div className="warehouse-bottom-body">
            <nav className="warehouse-bottom-tabs" aria-label={de ? "Arbeitsmodus" : "Work mode"}>
              {[
                ["slot", de ? "Slot editieren" : "Edit slot"],
                ["stock", de ? "Bestand" : "Stock"],
                ["count", de ? "Inventur" : "Count"]
              ].map(([id, label]) => (
                <button className={mode === id ? "is-active" : ""} key={id} onClick={() => setMode(id as typeof mode)} type="button">{label}</button>
              ))}
            </nav>
            {selectedSlot ? (
              <div className="warehouse-bottom-grid">
                <form
                  className="warehouse-bottom-editor"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    void runLayout("renameLocation", {
                      locationName: String(form.get("locationName") ?? selectedSlot.name),
                      parentId: selectedSlot.id
                    }, de ? "Slot umbenannt" : "Slot renamed");
                  }}
                >
                  <label>
                    <span>{de ? "Name" : "Name"}</span>
                    <input name="locationName" defaultValue={selectedSlot.name} />
                  </label>
                  <label>
                    <span>Status</span>
                    <input readOnly value={selectedSlot.pickable ? (de ? "pickbar" : "pickable") : (de ? "gesperrt" : "blocked")} />
                  </label>
                  <button disabled={busy !== null} type="submit">{de ? "Speichern" : "Save"}</button>
                  <button disabled={busy !== null} onClick={() => void runLayout("toggleLocationPickable", { parentId: selectedSlot.id }, de ? "Status geaendert" : "Status changed")} type="button">
                    {selectedSlot.pickable ? (de ? "Sperren" : "Block") : (de ? "Entsperren" : "Unblock")}
                  </button>
                  <button disabled={busy !== null} onClick={() => void runLayout("duplicateLocation", { parentId: selectedSlot.id }, de ? "Slot dupliziert" : "Slot duplicated")} type="button">
                    {de ? "Duplizieren" : "Duplicate"}
                  </button>
                </form>
                <div className="warehouse-bottom-stock">
                  {(mode === "slot" ? selectedSlotBalances.slice(0, 3) : selectedSlotBalances).map((balance) => (
                    <a href={panelHref("balance", balance.balanceKey, "right")} key={balance.balanceKey}>
                      <span><strong>{itemName(balance.inventoryItemId)}</strong><small>{balance.stockStatus} · {balance.inventoryOwnerPartyId}</small></span>
                      <em>{balance.quantity}</em>
                    </a>
                  ))}
                  {selectedSlotBalances.length === 0 ? <span className="warehouse-empty-note">{de ? "Dieser Slot ist leer." : "This slot is empty."}</span> : null}
                </div>
                <div className="warehouse-bottom-ai">
                  <strong>{de ? "CTOX Aktionen" : "CTOX actions"}</strong>
                  <a href={panelHref("warehouse-admin", selectedSlot.id, "left-bottom")}>{de ? "Stammdaten links unten" : "Master data lower-left"}</a>
                  <a href={panelHref("location", selectedSlot.id, "right")}>{de ? "Audit rechts" : "Audit right"}</a>
                  <span>{de ? "Rechtsklick auf Slot oder Bestand oeffnet kontextuelle CTOX-Aktionen." : "Right-click slot or stock for contextual CTOX actions."}</span>
                </div>
              </div>
            ) : (
              <div className="warehouse-bottom-empty">
                <strong>{de ? "Slot im Lagerplan auswaehlen" : "Select a slot in the warehouse map"}</strong>
                <span>{de ? "Dann erscheinen Editieren, Bestand, Inventur und Kontextaktionen unten." : "Edit, stock, count, and context actions appear here."}</span>
              </div>
            )}
          </div>
        ) : null}
      </aside>
    </div>
  );
}
