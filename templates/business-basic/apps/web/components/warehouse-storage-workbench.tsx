"use client";

import { useEffect, useMemo, useState } from "react";
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

type WarehouseContextActionDetail = {
  actionId?: string;
  item?: {
    moduleId?: string;
    recordId?: string;
    recordType?: string;
    submoduleId?: string;
  };
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
  const [selectedItemId, setSelectedItemId] = useState<string | undefined>();
  const [selectedStructureLocationId, setSelectedStructureLocationId] = useState<string | undefined>();
  const [focusType, setFocusType] = useState<"slot" | "item" | "location">("slot");
  const [bottomOpen, setBottomOpen] = useState(false);
  const [mode, setMode] = useState<"slot" | "stock" | "count" | "audit" | "item" | "location">("slot");
  const [search, setSearch] = useState(query.warehouseSearch ?? "");
  const [dragBalanceKey, setDragBalanceKey] = useState<string | null>(null);
  const [pendingMove, setPendingMove] = useState<{ balanceKey: string; targetLocationId: string } | undefined>();
  const [structureSlotCount, setStructureSlotCount] = useState(4);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState(de ? "Bereit" : "Ready");

  const warehouses = snapshot.locations.filter((location) => location.kind === "warehouse");
  const selectedWarehouse = warehouses.find((location) => location.id === selectedWarehouseId) ?? warehouses[0];
  const childLocations = (parentId: string) => snapshot.locations.filter((location) => location.parentId === parentId);
  const descendantIds = (parentId: string): string[] => childLocations(parentId).flatMap((location) => [location.id, ...descendantIds(location.id)]);
  const selectedLocationIds = new Set(selectedWarehouse ? [selectedWarehouse.id, ...descendantIds(selectedWarehouse.id)] : []);
  const selectedSlot = selectedSlotId ? snapshot.locations.find((location) => location.id === selectedSlotId) : undefined;
  const selectedItem = selectedItemId ? snapshot.items.find((item) => item.id === selectedItemId) : undefined;
  const selectedStructureLocation = focusType === "location"
    ? snapshot.locations.find((location) => location.id === selectedStructureLocationId) ?? selectedWarehouse
    : undefined;
  const selectedSlotBalances = selectedSlot ? snapshot.balances.filter((balance) => balance.locationId === selectedSlot.id && balance.quantity > 0 && activeStatuses.includes(balance.stockStatus)) : [];
  const selectedOpenCycleCount = selectedSlot ? snapshot.cycleCounts.find((count) => count.locationId === selectedSlot.id && count.status === "open") : undefined;
  const selectedLatestCycleCount = selectedSlot
    ? snapshot.cycleCounts.filter((count) => count.locationId === selectedSlot.id).sort((a, b) => b.openedAt.localeCompare(a.openedAt))[0]
    : undefined;
  const selectedMoveBalance = pendingMove ? snapshot.balances.find((balance) => balance.balanceKey === pendingMove.balanceKey) : undefined;
  const selectedAuditMovements = selectedSlot
    ? snapshot.movements
        .filter((movement) => movement.locationId === selectedSlot.id)
        .sort((a, b) => b.postedAt.localeCompare(a.postedAt))
        .slice(0, 8)
    : [];
  const visibleWarehouses = warehouses.filter((warehouse) => {
    const needle = search.trim().toLowerCase();
    if (!needle) return true;
    const ids = new Set([warehouse.id, ...descendantIds(warehouse.id)]);
    const locationNames = [warehouse.name, ...descendantIds(warehouse.id).map((id) => locationName(id))].join(" ");
    const stockContext = snapshot.balances
      .filter((balance) => ids.has(balance.locationId))
      .map((balance) => `${itemName(balance.inventoryItemId)} ${balance.inventoryOwnerPartyId} ${balance.stockStatus}`)
      .join(" ");
    return `${locationNames} ${stockContext}`.toLowerCase().includes(needle);
  });

  const zones = selectedWarehouse
    ? childLocations(selectedWarehouse.id)
        .filter((location) => location.kind === "zone")
        .map((zone) => ({
          slots: childLocations(zone.id).filter((location) => location.kind === "bin"),
          zone
        }))
    : [];
  const warehouseSlots = zones.flatMap(({ slots }) => slots);

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
  const recentReceipts = snapshot.receipts
    .filter((receipt) => receipt.lines.some((line) => selectedLocationIds.has(line.locationId) || snapshot.putawayTasks.some((task) => task.receiptLineId === line.id && selectedLocationIds.has(task.toLocationId))))
    .slice(-3)
    .reverse();
  const openPutawayRows = snapshot.putawayTasks
    .filter((task) => task.status === "open" && selectedLocationIds.has(task.toLocationId))
    .slice(0, 8);
  const qualityHoldRows = snapshot.balances
    .filter((balance) =>
      balance.quantity > 0 &&
      (balance.stockStatus === "quarantine" || balance.stockStatus === "damaged") &&
      (balance.locationId === "loc-receiving" || selectedLocationIds.has(balance.locationId))
    )
    .sort((a, b) => b.quantity - a.quantity)
    .slice(0, 6);
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
  const pickTaskRows = snapshot.pickLists
    .slice()
    .reverse()
    .slice(0, 5);
  const openWaveReservations = snapshot.reservations.filter((reservation) => reservation.status !== "consumed" && reservation.status !== "cancelled" && reservation.status !== "released");
  const transferRows = snapshot.transfers.slice().reverse().slice(0, 3);
  const packableShipments = snapshot.shipments.filter((shipment) => !snapshot.shipmentPackages.some((pkg) => pkg.shipmentId === shipment.id));
  const packedPackages = snapshot.shipmentPackages.filter((pkg) => pkg.status === "packed");
  const labelledPackages = snapshot.shipmentPackages.filter((pkg) => pkg.status === "labelled");
  const returnableShipments = snapshot.shipments.filter((shipment) => shipment.status === "shipped" && shipment.lines.length > 0 && !snapshot.returns.some((ret) => ret.sourceShipmentId === shipment.id));
  const authorizedReturns = snapshot.returns.filter((entry) => entry.status === "authorized");
  const lowStockRows = snapshot.items
    .map((item) => ({
      available: snapshot.balances.filter((balance) => balance.inventoryItemId === item.id && balance.stockStatus === "available").reduce((sum, balance) => sum + balance.quantity, 0),
      item
    }))
    .filter((row) => row.available < 5)
    .slice(0, 4);
  const riskRows = [
    ...openPutawayRows.map((task) => ({ id: task.id, label: `${itemName(task.inventoryItemId)} -> ${locationName(task.toLocationId)}`, type: de ? "Einlagerung offen" : "Open putaway" })),
    ...pickTaskRows.filter((pickList) => pickList.status === "ready").map((pickList) => ({ id: pickList.id, label: `${pickList.lines.length} ${de ? "Pickzeilen" : "pick lines"}`, type: de ? "Pick wartet" : "Pick waiting" })),
    ...qualityHoldRows.map((balance) => ({ id: balance.balanceKey, label: `${itemName(balance.inventoryItemId)} · ${balance.quantity}`, type: de ? "QS/Sperrbestand" : "Quality hold" }))
  ].slice(0, 5);
  const inactiveItemIds = new Set(snapshot.commandLog
    .filter((command) => command.refType === "inventory_item" && typeof command.payload === "object" && command.payload && "status" in command.payload && command.payload.status === "inactive")
    .map((command) => command.refId));
  const inactiveLocationIds = new Set<string>();
  snapshot.commandLog
    .filter((command) =>
      command.refType === "warehouse_location" &&
      typeof command.payload === "object" &&
      command.payload &&
      (command.payload.status === "inactive" || command.payload.status === "active")
    )
    .forEach((command) => {
      const affected = command.payload.affectedLocationIds;
      const ids = Array.isArray(affected) ? affected.filter((id): id is string => typeof id === "string") : [command.refId];
      ids.forEach((id) => {
        if (command.payload.status === "inactive") inactiveLocationIds.add(id);
        if (command.payload.status === "active") inactiveLocationIds.delete(id);
      });
    });
  const itemRows = snapshot.items
    .filter((item) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return `${item.name} ${item.sku} ${item.uom} ${item.trackingMode}`.toLowerCase().includes(needle);
    })
    .sort((a, b) => a.sku.localeCompare(b.sku))
    .slice(0, 10);
  const activeItemRows = snapshot.items.filter((item) => !inactiveItemIds.has(item.id)).sort((a, b) => a.sku.localeCompare(b.sku));
  const ownerOptions = Array.from(new Set([
    selectedWarehouse?.defaultOwnerPartyId,
    "owner-system",
    "cust-nova",
    ...snapshot.balances.map((balance) => balance.inventoryOwnerPartyId),
    ...snapshot.receipts.map((receipt) => receipt.inventoryOwnerPartyId),
    ...snapshot.putawayTasks.map((task) => task.inventoryOwnerPartyId)
  ].filter((id): id is string => Boolean(id))));

  function itemName(id: string) {
    return snapshot.items.find((item) => item.id === id)?.name ?? id;
  }

  function ownerName(id: string) {
    if (id === "owner-system") return de ? "Eigenbestand" : "Own stock";
    return id.replace(/^cust-/, "");
  }

  function locationName(id: string) {
    return snapshot.locations.find((location) => location.id === id)?.name ?? id;
  }

  function slotQuantity(slotId: string, statuses: StockStatus[] = activeStatuses) {
    return snapshot.balances
      .filter((balance) => balance.locationId === slotId && balance.quantity > 0 && statuses.includes(balance.stockStatus))
      .reduce((sum, balance) => sum + balance.quantity, 0);
  }

  function slotCapacityLabel(slot: WarehouseLocation) {
    const quantity = slotQuantity(slot.id);
    return slot.capacityUnits ? `${quantity}/${slot.capacityUnits}` : String(quantity || (de ? "frei" : "free"));
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

  function previewSectionCode(warehouseId: string) {
    const existing = childLocations(warehouseId).filter((location) => location.kind === "zone").length;
    return String.fromCharCode(65 + existing);
  }

  function previewSlots(zone: WarehouseLocation, count: number) {
    const existingSlots = childLocations(zone.id).filter((location) => location.kind === "bin");
    const safeCount = Math.max(1, Math.min(24, count || 4));
    const prefix = zone.name.match(/[A-Z]/)?.[0] ?? "S";
    return Array.from({ length: safeCount }, (_, index) => {
      const number = existingSlots.length + index + 1;
      return {
        bay: String(number),
        capacity: 100,
        level: "1",
        name: `${prefix}${number}`,
        slotType: "pick_face"
      };
    });
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
    setFocusType("slot");
    setMode("slot");
    setBottomOpen(true);
  }

  function prepareMove(balanceKey: string, targetLocationId: string) {
    const balance = snapshot.balances.find((entry) => entry.balanceKey === balanceKey);
    if (!balance || balance.locationId === targetLocationId) return;
    setPendingMove({ balanceKey, targetLocationId });
    setSelectedSlotId(targetLocationId);
    setFocusType("slot");
    setMode("stock");
    setBottomOpen(true);
    setMessage(de ? "Umlagerung pruefen" : "Review stock move");
  }

  function selectItem(itemId: string) {
    setSelectedItemId(itemId);
    setFocusType("item");
    setMode("item");
    setBottomOpen(true);
  }

  function selectStructureLocation(locationId: string) {
    setSelectedStructureLocationId(locationId);
    setSelectedSlotId(undefined);
    setSelectedItemId(undefined);
    setFocusType("location");
    setMode("location");
    setBottomOpen(true);
  }

  function selectedBalanceLabel(balance: StockBalance) {
    return `${itemName(balance.inventoryItemId)} · ${ownerName(balance.inventoryOwnerPartyId)} · ${balance.quantity} · ${balance.stockStatus}`;
  }

  function receiptProgressLabel(receiptId: string) {
    const command = snapshot.commandLog.find((entry) => entry.refType === "warehouse_receipt" && entry.refId === receiptId && entry.type === "ReceiveStock");
    const payload = command?.payload;
    if (!payload || typeof payload !== "object") return undefined;
    const expected = typeof payload.expectedQuantity === "number" ? payload.expectedQuantity : undefined;
    const accepted = typeof payload.acceptedQuantity === "number" ? payload.acceptedQuantity : undefined;
    const damaged = typeof payload.damagedQuantity === "number" ? payload.damagedQuantity : 0;
    const disposition = payload.receiptDisposition === "damaged" ? (de ? "Schaden" : "damaged") : (de ? "Sperre" : "hold");
    if (expected === undefined || accepted === undefined) return undefined;
    const total = accepted + damaged;
    const variance = total - expected;
    const varianceLabel = variance === 0 ? (de ? "vollstaendig" : "complete") : variance > 0 ? `+${variance}` : String(variance);
    return `${accepted}/${expected} ${de ? "OK" : "ok"}${damaged > 0 ? ` · ${damaged} ${disposition}` : ""} · ${varianceLabel}`;
  }

  useEffect(() => {
    const onContextAction = (event: Event) => {
      const detail = (event as CustomEvent<WarehouseContextActionDetail>).detail;
      const actionId = detail?.actionId;
      const item = detail?.item;
      const recordId = item?.recordId;
      if (!actionId || !recordId || item?.moduleId !== "business" || item.submoduleId !== submoduleId) return;

      if (actionId === "warehouse-source-edit") {
        setSelectedWarehouseId(recordId);
        selectStructureLocation(recordId);
        setMessage(de ? "Lager bearbeiten" : "Edit warehouse");
      } else if (actionId === "warehouse-source-section") {
        setSelectedWarehouseId(recordId);
        void runLayout("createSection", { parentId: recordId }, de ? "Bereich angelegt" : "Section added");
      } else if (actionId === "warehouse-source-duplicate") {
        void runLayout("duplicateLocation", { parentId: recordId }, de ? "Lager dupliziert" : "Warehouse duplicated");
      } else if (actionId === "warehouse-source-toggle") {
        void runLayout("toggleLocationActive", { parentId: recordId }, de ? "Lagerstatus geaendert" : "Warehouse status changed");
      } else if (actionId === "warehouse-zone-edit") {
        selectStructureLocation(recordId);
        setMessage(de ? "Zone bearbeiten" : "Edit zone");
      } else if (actionId === "warehouse-zone-slots") {
        void runLayout("createSlot", { parentId: recordId, slotCount: 4 }, de ? "Slots angelegt" : "Slots added");
      } else if (actionId === "warehouse-zone-duplicate") {
        void runLayout("duplicateLocation", { parentId: recordId }, de ? "Zone dupliziert" : "Zone duplicated");
      } else if (actionId === "warehouse-slot-edit") {
        selectSlot(recordId);
        setMode("slot");
        setMessage(de ? "Slot bearbeiten" : "Edit slot");
      } else if (actionId === "warehouse-slot-duplicate") {
        void runLayout("duplicateLocation", { parentId: recordId }, de ? "Slot dupliziert" : "Slot duplicated");
      } else if (actionId === "warehouse-slot-block") {
        selectSlot(recordId);
        void runLayout("toggleLocationPickable", { parentId: recordId }, de ? "Slotstatus geaendert" : "Slot status changed");
      } else if (actionId === "warehouse-slot-count") {
        selectSlot(recordId);
        setMode("count");
        setMessage(de ? "Inventurmodus" : "Count mode");
      } else if (actionId === "warehouse-slot-audit") {
        selectSlot(recordId);
        setMode("audit");
        setMessage(de ? "Auditmodus" : "Audit mode");
      } else if (actionId === "warehouse-item-edit") {
        selectItem(recordId);
        setMessage(de ? "Artikel bearbeiten" : "Edit item");
      } else if (actionId === "warehouse-item-duplicate") {
        void runLayout("duplicateItem", { inventoryItemId: recordId }, de ? "Artikel dupliziert" : "Item duplicated");
      } else if (actionId === "warehouse-item-deactivate") {
        void runLayout("deactivateItem", { inventoryItemId: recordId }, de ? "Artikel deaktiviert" : "Item deactivated");
      } else if (actionId === "warehouse-stock-reserve") {
        void runLayout("reserveBalance", {
          balanceKey: recordId,
          quantity: 1,
          sourceId: `manual-${new Date().toISOString().slice(0, 10)}`
        }, de ? "In Ausgang reserviert" : "Reserved for outbound");
      } else if (actionId === "warehouse-stock-move" || actionId === "warehouse-stock-audit") {
        const balance = snapshot.balances.find((entry) => entry.balanceKey === recordId);
        if (!balance) return;
        selectSlot(balance.locationId);
        setMode(actionId === "warehouse-stock-audit" ? "audit" : "stock");
        if (actionId === "warehouse-stock-move") setPendingMove({ balanceKey: balance.balanceKey, targetLocationId: balance.locationId });
        setMessage(actionId === "warehouse-stock-audit" ? (de ? "Auditmodus" : "Audit mode") : (de ? "Umlagerung vorbereiten" : "Prepare move"));
      }
    };

    window.addEventListener("ctox:context-action", onContextAction);
    return () => window.removeEventListener("ctox:context-action", onContextAction);
  }, [de, snapshot.balances, submoduleId]);

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
          <input className="warehouse-search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={de ? "Lager, Slot oder Artikel suchen" : "Find warehouse, slot or item"} />
          <button className="warehouse-subtle-action" disabled={busy !== null} onClick={() => void runLayout("createWarehouse", { locationName: search.trim() || undefined }, de ? "Lager angelegt" : "Warehouse created")} type="button">
            {de ? "Neu" : "New"}
          </button>
        </div>
        <div className="warehouse-source-list">
          {visibleWarehouses.map((warehouse) => {
            const ids = new Set([warehouse.id, ...descendantIds(warehouse.id)]);
            const available = snapshot.balances.filter((balance) => ids.has(balance.locationId) && balance.stockStatus === "available").reduce((sum, balance) => sum + balance.quantity, 0);
            const sections = childLocations(warehouse.id).filter((location) => location.kind === "zone").length;
            const inactive = inactiveLocationIds.has(warehouse.id);
            return (
              <button
                aria-current={warehouse.id === selectedWarehouse?.id ? "page" : undefined}
                className={`${warehouse.id === selectedWarehouse?.id ? "warehouse-source-card is-active" : "warehouse-source-card"} ${inactive ? "is-inactive" : ""}`}
                data-context-item
                data-context-label={warehouse.name}
                data-context-module="business"
                data-context-record-id={warehouse.id}
                data-context-record-type="warehouse_source"
                data-context-submodule={submoduleId}
                key={warehouse.id}
                onClick={() => {
                  setSelectedWarehouseId(warehouse.id);
                  selectStructureLocation(warehouse.id);
                }}
                type="button"
              >
                <span className="warehouse-avatar">{warehouse.name.slice(0, 1)}</span>
                <span><strong>{warehouse.name}</strong><small>{sections} {de ? "Bereiche" : "areas"}{inactive ? ` · ${de ? "inaktiv" : "inactive"}` : ""}</small></span>
                <em>{available}</em>
              </button>
            );
          })}
        </div>
        <div className="warehouse-left-metrics">
          {kpis.map(([label, value]) => <div key={label}><span>{label}</span><strong>{value}</strong></div>)}
        </div>
        <form
          className="warehouse-inbound-form"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            const quantity = Number(form.get("quantity"));
            const expectedQuantity = Number(form.get("expectedQuantity"));
            const damagedQuantity = Number(form.get("damagedQuantity"));
            void runLayout("receiveInbound", {
              damagedQuantity: Number.isFinite(damagedQuantity) ? damagedQuantity : undefined,
              expectedQuantity: Number.isFinite(expectedQuantity) ? expectedQuantity : undefined,
              inventoryItemId: String(form.get("inventoryItemId") ?? ""),
              inventoryOwnerPartyId: String(form.get("inventoryOwnerPartyId") ?? "owner-system"),
              lotId: String(form.get("lotId") ?? "").trim() || undefined,
              quantity: Number.isFinite(quantity) ? quantity : undefined,
              receiptDisposition: String(form.get("receiptDisposition") ?? "quarantine") === "damaged" ? "damaged" : "quarantine",
              serialId: String(form.get("serialId") ?? "").trim() || undefined,
              sourceId: String(form.get("sourceId") ?? "").trim() || undefined,
              targetLocationId: String(form.get("targetLocationId") ?? "")
            }, de ? "Wareneingang gebucht" : "Inbound received");
          }}
        >
          <header>
            <strong>{de ? "Neue Annahme" : "New receipt"}</strong>
            <span>{de ? "bucht zuerst in Wareneingang" : "posts into receiving first"}</span>
          </header>
          <label>
            <span>{de ? "Artikel" : "Item"}</span>
            <select name="inventoryItemId" defaultValue={activeItemRows[0]?.id ?? ""}>
              {activeItemRows.map((item) => (
                <option key={item.id} value={item.id}>{item.sku} · {item.name}</option>
              ))}
            </select>
          </label>
          <label>
            <span>Owner</span>
            <select name="inventoryOwnerPartyId" defaultValue={selectedWarehouse?.defaultOwnerPartyId ?? "owner-system"}>
              {ownerOptions.map((ownerId) => <option key={ownerId} value={ownerId}>{ownerName(ownerId)}</option>)}
            </select>
          </label>
          <label>
            <span>{de ? "Erwartet" : "Expected"}</span>
            <input min="0" name="expectedQuantity" type="number" defaultValue={1} />
          </label>
          <label>
            <span>{de ? "OK-Menge" : "Accepted"}</span>
            <input min="0" name="quantity" type="number" defaultValue={1} />
          </label>
          <label>
            <span>{de ? "Sperre/Schaden" : "Hold/Damage"}</span>
            <input min="0" name="damagedQuantity" type="number" defaultValue={0} />
          </label>
          <label>
            <span>{de ? "Status Ausnahme" : "Exception status"}</span>
            <select name="receiptDisposition" defaultValue="quarantine">
              <option value="quarantine">{de ? "QS/Sperrbestand" : "QA hold"}</option>
              <option value="damaged">{de ? "Beschaedigt" : "Damaged"}</option>
            </select>
          </label>
          <label>
            <span>{de ? "Zielslot" : "Target slot"}</span>
            <select name="targetLocationId" defaultValue={warehouseSlots[0]?.id ?? ""}>
              {warehouseSlots.map((slot) => <option key={slot.id} value={slot.id}>{slot.name} · {locationName(slot.parentId ?? "")}</option>)}
            </select>
          </label>
          <label>
            <span>Lot</span>
            <input name="lotId" placeholder={de ? "falls chargengefuehrt" : "if lot-tracked"} />
          </label>
          <label>
            <span>Serial</span>
            <input name="serialId" placeholder={de ? "falls seriengefuehrt" : "if serial-tracked"} />
          </label>
          <label className="warehouse-inbound-source">
            <span>{de ? "Beleg" : "Source"}</span>
            <input name="sourceId" placeholder={de ? "PO, Retoure, manuell" : "PO, return, manual"} />
          </label>
          <button disabled={busy !== null || !activeItemRows.length || !warehouseSlots.length} type="submit">
            {de ? "Annehmen" : "Receive"}
          </button>
        </form>
        <section className="warehouse-flow-panel">
          <header>
            <strong>{de ? "Einlagerung" : "Putaway"}</strong>
            <button disabled={busy !== null || !selectedWarehouse} onClick={() => selectedWarehouse ? void runLayout("createSection", { parentId: selectedWarehouse.id }, de ? "Bereich angelegt" : "Section added") : undefined} type="button">
              {de ? "Bereich" : "Section"}
            </button>
          </header>
          <div>
            {openPutawayRows.map((task) => (
              <form
                className="warehouse-flow-item warehouse-putaway-row warehouse-scan-putaway-row"
                data-context-item
                data-context-label={`${itemName(task.inventoryItemId)} ${task.quantity}`}
                data-context-module="business"
                data-context-record-id={task.id}
                data-context-record-type="warehouse_putaway"
                data-context-submodule={submoduleId}
                key={task.id}
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget);
                  const submitter = (event.nativeEvent as SubmitEvent).submitter;
                  const intent = submitter instanceof HTMLButtonElement ? submitter.value : "scan";
                  if (intent === "manual") {
                    void runLayout("completePutaway", { putawayTaskId: task.id }, de ? "Einlagerung abgeschlossen" : "Putaway completed");
                    return;
                  }
                  void runLayout("scanPutaway", {
                    putawayTaskId: task.id,
                    scanBarcode: String(form.get("scanBarcode") ?? "").trim(),
                    scannerDeviceId: String(form.get("scannerDeviceId") ?? "web-scanner").trim()
                  }, de ? "Scan eingelagert" : "Scan putaway completed");
                }}
              >
                <span><strong>{itemName(task.inventoryItemId)}</strong><small>{ownerName(task.inventoryOwnerPartyId)} · {task.quantity} {de ? "nach" : "to"} {locationName(task.toLocationId)}</small></span>
                <label>
                  <span>{de ? "Scan" : "Scan"}</span>
                  <input name="scanBarcode" placeholder={`${snapshot.items.find((item) => item.id === task.inventoryItemId)?.sku ?? task.inventoryItemId} / ${locationName(task.toLocationId)}`} />
                </label>
                  <input name="scannerDeviceId" type="hidden" value="web-scanner" />
                  <span className="warehouse-putaway-actions">
                    <button
                      disabled={busy !== null}
                      name="intent"
                      onClick={(event) => {
                        event.preventDefault();
                        const form = event.currentTarget.form;
                        if (!form) return;
                        const formData = new FormData(form);
                        void runLayout("scanPutaway", {
                          putawayTaskId: task.id,
                          scanBarcode: String(formData.get("scanBarcode") ?? "").trim(),
                          scannerDeviceId: String(formData.get("scannerDeviceId") ?? "web-scanner").trim()
                        }, de ? "Scan eingelagert" : "Scan put away");
                      }}
                      type="submit"
                      value="scan"
                    >
                      {de ? "Scan" : "Scan"}
                    </button>
                    <button
                      disabled={busy !== null}
                      name="intent"
                      onClick={(event) => {
                        event.preventDefault();
                        void runLayout("completePutaway", { putawayTaskId: task.id }, de ? "Eingelagert" : "Put away");
                      }}
                      type="submit"
                      value="manual"
                    >
                      {de ? "Manuell" : "Manual"}
                    </button>
                  </span>
                </form>
              ))}
            {recentReceipts.map((receipt) => (
              <a className="warehouse-flow-item" data-context-item data-context-label={receipt.sourceId} data-context-module="business" data-context-record-id={receipt.id} data-context-record-type="warehouse_receipt" data-context-submodule={submoduleId} href={panelHref("receipt", receipt.id, "left-bottom")} key={receipt.id}>
                <span><strong>{receipt.sourceId}</strong><small>{receipt.status} · {receiptProgressLabel(receipt.id) ?? `${receipt.lines.length} ${de ? "Positionen" : "lines"}`}</small></span>
              </a>
            ))}
            {openPutawayRows.length === 0 && recentReceipts.length === 0 ? <span className="warehouse-empty-note">{de ? "Keine offenen Wareneingaenge." : "No inbound work."}</span> : null}
          </div>
        </section>
        <section className="warehouse-flow-panel warehouse-quality-panel">
          <header>
            <strong>{de ? "QS-Pruefung" : "QA review"}</strong>
            <span>{qualityHoldRows.reduce((sum, balance) => sum + balance.quantity, 0)}</span>
          </header>
          <div>
            {qualityHoldRows.map((balance) => (
              <form
                className="warehouse-flow-item warehouse-quality-row"
                data-context-item
                data-context-label={selectedBalanceLabel(balance)}
                data-context-module="business"
                data-context-record-id={balance.balanceKey}
                data-context-record-type="stock_balance"
                data-context-submodule={submoduleId}
                key={balance.balanceKey}
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget);
                  const submitter = (event.nativeEvent as SubmitEvent).submitter;
                  const intent = submitter instanceof HTMLButtonElement ? submitter.value : String(form.get("intent") ?? "release");
                  const quantity = Number(form.get("quantity"));
                  if (intent === "scrap") {
                    void runLayout("scrapQualityHold", {
                      balanceKey: balance.balanceKey,
                      quantity: Number.isFinite(quantity) ? quantity : undefined,
                      reasonCode: String(form.get("reasonCode") ?? "qa_scrap")
                    }, de ? "QS-Bestand ausgebucht" : "QA stock scrapped");
                    return;
                  }
                  void runLayout("resolveQualityHold", {
                    balanceKey: balance.balanceKey,
                    quantity: Number.isFinite(quantity) ? quantity : undefined,
                    reasonCode: String(form.get("reasonCode") ?? "qa_release"),
                    targetLocationId: String(form.get("targetLocationId") ?? "")
                  }, de ? "QS-Bestand freigegeben" : "QA stock released");
                }}
              >
                <span>
                  <strong>{itemName(balance.inventoryItemId)}</strong>
                  <small>{ownerName(balance.inventoryOwnerPartyId)} · {locationName(balance.locationId)} · {balance.stockStatus}</small>
                </span>
                <em>{balance.quantity}</em>
                <label>
                  <span>{de ? "Menge" : "Qty"}</span>
                  <input min="1" max={balance.quantity} name="quantity" type="number" defaultValue={balance.quantity} />
                </label>
                <label>
                  <span>{de ? "Zielslot" : "Target slot"}</span>
                  <select name="targetLocationId" defaultValue={warehouseSlots[0]?.id ?? ""}>
                    {warehouseSlots.map((slot) => <option key={slot.id} value={slot.id}>{slot.name} · {locationName(slot.parentId ?? "")}</option>)}
                  </select>
                </label>
                <label>
                  <span>{de ? "Grund" : "Reason"}</span>
                  <select name="reasonCode" defaultValue={balance.stockStatus === "damaged" ? "qa_rework_ok" : "qa_release"}>
                    <option value="qa_release">{de ? "QS freigegeben" : "QA released"}</option>
                    <option value="qa_rework_ok">{de ? "Nacharbeit OK" : "Rework passed"}</option>
                    <option value="qa_scrap">{de ? "Ausschuss" : "Scrap"}</option>
                  </select>
                </label>
                <span className="warehouse-quality-actions">
                  <button disabled={busy !== null || !warehouseSlots.length} name="intent" type="submit" value="release">{de ? "Freigeben" : "Release"}</button>
                  <button disabled={busy !== null} name="intent" type="submit" value="scrap">{de ? "Ausschuss" : "Scrap"}</button>
                </span>
              </form>
            ))}
            {qualityHoldRows.length === 0 ? <span className="warehouse-empty-note">{de ? "Keine QS-Ausnahmen." : "No QA exceptions."}</span> : null}
          </div>
        </section>
        <section className="warehouse-flow-panel warehouse-item-master-panel">
          <header>
            <strong>{de ? "Artikelstamm" : "Item master"}</strong>
            <button disabled={busy !== null} onClick={() => void runLayout("createItem", {}, de ? "Artikel angelegt" : "Item created")} type="button">
              {de ? "Artikel" : "Item"}
            </button>
          </header>
          <div>
            {itemRows.map((item) => {
              const quantity = snapshot.balances.filter((balance) => balance.inventoryItemId === item.id).reduce((sum, balance) => sum + balance.quantity, 0);
              const inactive = inactiveItemIds.has(item.id);
              return (
                <button
                  className={`warehouse-flow-item warehouse-item-master-row ${item.id === selectedItemId && focusType === "item" ? "is-selected" : ""} ${inactive ? "is-inactive" : ""}`}
                  data-context-item
                  data-context-label={`${item.name} ${item.sku}`}
                  data-context-module="business"
                  data-context-record-id={item.id}
                  data-context-record-type="inventory_item"
                  data-context-submodule={submoduleId}
                  key={item.id}
                  onClick={() => selectItem(item.id)}
                  type="button"
                >
                  <span>
                    <strong>{item.name}</strong>
                    <small>{item.sku} · {item.trackingMode} · {item.uom}{inactive ? ` · ${de ? "deaktiviert" : "inactive"}` : ""}</small>
                  </span>
                  <em>{quantity}</em>
                </button>
              );
            })}
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
              <article className="warehouse-zone-card warehouse-zone-lane" data-context-item data-context-label={zone.name} data-context-module="business" data-context-record-id={zone.id} data-context-record-type="warehouse_zone" data-context-submodule={submoduleId} key={zone.id}>
                <header>
                  <span><strong>{zone.name}</strong><small>{used}/{slots.length} {de ? "Slots genutzt" : "slots used"}</small></span>
                  <button disabled={busy !== null} onClick={() => selectStructureLocation(zone.id)} type="button">{de ? "Edit" : "Edit"}</button>
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
                            prepareMove(dragBalanceKey, slot.id);
                            setDragBalanceKey(null);
                          }
                        }}
                        type="button"
                      >
                        <strong>{slot.name}</strong>
                        <span>{slotCapacityLabel(slot)}</span>
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
                            {itemName(balance.inventoryItemId).slice(0, 8)} · {ownerName(balance.inventoryOwnerPartyId).slice(0, 8)}
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
                <div
                  className={`warehouse-source-match is-${row.stockStatus}`}
                  data-context-item
                  data-context-label={selectedBalanceLabel(row)}
                  data-context-module="business"
                  data-context-record-id={row.balanceKey}
                  data-context-record-type="stock_balance"
                  data-context-submodule={submoduleId}
                  key={row.balanceKey}
                >
                  <a href={panelHref("balance", row.balanceKey, "right")}>
                    <span><strong>{itemName(row.inventoryItemId)}</strong><small>{ownerName(row.inventoryOwnerPartyId)} · {locationName(row.locationId)} · {row.stockStatus}</small></span>
                    <em>{row.quantity}</em>
                  </a>
                  <span className="warehouse-cart-actions">
                    <button
                      disabled={busy !== null || row.stockStatus !== "available"}
                      onClick={() => void runLayout("reserveBalance", {
                        balanceKey: row.balanceKey,
                        quantity: 1,
                        sourceId: `manual-${new Date().toISOString().slice(0, 10)}`
                      }, de ? "In Ausgang reserviert" : "Reserved for outbound")}
                      type="button"
                    >
                      {de ? "+ Ausgang" : "+ Out"}
                    </button>
                  </span>
                </div>
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
                  <div className="warehouse-source-match warehouse-cart-order" data-context-item data-context-label={reservation.sourceId} data-context-module="business" data-context-record-id={id} data-context-record-type="warehouse_reservation" data-context-submodule={submoduleId} key={id}>
                    <a href={panelHref("reservation", id, "right")}><span><strong>{reservation.sourceId}</strong><small>{picked}/{quantity} {de ? "gepickt" : "picked"} · {reservation.status}</small></span></a>
                    <span className="warehouse-cart-actions">
                      <button disabled={busy !== null || !canPick} onClick={() => void runLayout("createPickList", { reservationId: id }, de ? "Pickliste bereit" : "Pick list ready")} type="button">{de ? "Liste" : "List"}</button>
                      <button disabled={busy !== null || !canPick} onClick={() => void runReservation("pick", id)} type="button">{de ? "Pick" : "Pick"}</button>
                      <button disabled={busy !== null || !canShip} onClick={() => void runReservation("ship", id)} type="button">{de ? "Ship" : "Ship"}</button>
                    </span>
                  </div>
                );
              })}
            </div>
          </section>
          <section className="warehouse-match-panel warehouse-cart-panel warehouse-pick-task-panel">
            <h3>{de ? "Pick-Aufgaben" : "Pick tasks"}</h3>
            <div className="warehouse-source-match-list">
              {pickTaskRows.map((pickList) => (
                <form
                  className="warehouse-source-match warehouse-pick-task"
                  data-context-item
                  data-context-label={pickList.id}
                  data-context-module="business"
                  data-context-record-id={pickList.id}
                  data-context-record-type="warehouse_pick_list"
                  data-context-submodule={submoduleId}
                  key={pickList.id}
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    const submitter = (event.nativeEvent as SubmitEvent).submitter;
                    const intent = submitter instanceof HTMLButtonElement ? submitter.value : "scan";
                    if (intent === "manual") {
                      void runReservation("pick", pickList.reservationId);
                      return;
                    }
                    void runLayout("scanPick", {
                      reservationId: pickList.reservationId,
                      scanBarcode: String(form.get("scanBarcode") ?? "").trim(),
                      scannerDeviceId: String(form.get("scannerDeviceId") ?? "web-scanner").trim()
                    }, de ? "Scan gepickt" : "Scan picked");
                  }}
                >
                  <span>
                    <strong>{pickList.id.replace(/^pick-/, "")}</strong>
                    <small>{pickList.status} · {pickList.lines.length} {de ? "Zeilen" : "lines"}</small>
                  </span>
                  <em>{pickList.lines.reduce((sum, line) => sum + line.quantity, 0)}</em>
                  <i>
                    {pickList.lines.slice(0, 3).map((line) => (
                      <b key={line.id}>{locationName(line.locationId)} · {itemName(line.inventoryItemId)} · {line.quantity}</b>
                    ))}
                  </i>
                  {pickList.status === "ready" ? (
                    <>
                      <label>
                        <span>{de ? "Pick-Scan" : "Pick scan"}</span>
                        <input name="scanBarcode" placeholder={pickList.lines[0] ? `${itemName(pickList.lines[0].inventoryItemId)} / ${locationName(pickList.lines[0].locationId)}` : pickList.id} />
                      </label>
                      <input name="scannerDeviceId" type="hidden" value="web-scanner" />
                      <span className="warehouse-putaway-actions">
                        <button
                          disabled={busy !== null}
                          name="intent"
                          onClick={(event) => {
                            event.preventDefault();
                            const form = event.currentTarget.form;
                            if (!form) return;
                            const formData = new FormData(form);
                            void runLayout("scanPick", {
                              reservationId: pickList.reservationId,
                              scanBarcode: String(formData.get("scanBarcode") ?? "").trim(),
                              scannerDeviceId: String(formData.get("scannerDeviceId") ?? "web-scanner").trim()
                            }, de ? "Scan gepickt" : "Scan picked");
                          }}
                          type="submit"
                          value="scan"
                        >
                          {de ? "Scan" : "Scan"}
                        </button>
                        <button
                          disabled={busy !== null}
                          name="intent"
                          onClick={(event) => {
                            event.preventDefault();
                            void runReservation("pick", pickList.reservationId);
                          }}
                          type="submit"
                          value="manual"
                        >
                          {de ? "Manuell" : "Manual"}
                        </button>
                      </span>
                    </>
                  ) : (
                    <a href={panelHref("pick-list", pickList.id, "right")}>{de ? "Details" : "Details"}</a>
                  )}
                </form>
              ))}
              {pickTaskRows.length === 0 ? <span className="warehouse-empty-note">{de ? "Noch keine Picklisten." : "No pick lists yet."}</span> : null}
            </div>
          </section>
          <section className="warehouse-match-panel warehouse-cart-panel warehouse-ops-panel">
            <h3>{de ? "Welle, Transfer, Versand" : "Wave, transfer, shipping"}</h3>
            <div className="warehouse-ops-actions">
              <button disabled={busy !== null || openWaveReservations.length === 0} onClick={() => void runLayout("planWave", {}, de ? "Welle geplant" : "Wave planned")} type="button">{de ? "Welle planen" : "Plan wave"}</button>
              <button disabled={busy !== null || stockRows.length === 0} onClick={() => void runLayout("createInterWarehouseTransfer", { balanceKey: stockRows[0]?.balanceKey, quantity: 1 }, de ? "Umlagerung angelegt" : "Transfer created")} type="button">{de ? "Lagertransfer" : "Transfer"}</button>
              <button disabled={busy !== null || !transferRows.some((transfer) => transfer.status === "draft")} onClick={() => void runLayout("shipInterWarehouseTransfer", {}, de ? "Transfer versendet" : "Transfer shipped")} type="button">{de ? "Transfer raus" : "Ship transfer"}</button>
              <button disabled={busy !== null || !transferRows.some((transfer) => transfer.status === "shipped")} onClick={() => void runLayout("receiveInterWarehouseTransfer", {}, de ? "Transfer empfangen" : "Transfer received")} type="button">{de ? "Transfer rein" : "Receive transfer"}</button>
              <button disabled={busy !== null || packableShipments.length === 0} onClick={() => void runLayout("packShipment", {}, de ? "Packstueck erstellt" : "Package created")} type="button">{de ? "Packen" : "Pack"}</button>
              <button disabled={busy !== null || packedPackages.length === 0} onClick={() => void runLayout("createShipmentLabel", {}, de ? "Label erstellt" : "Label created")} type="button">{de ? "Label" : "Label"}</button>
              <button disabled={busy !== null || labelledPackages.length === 0} onClick={() => void runLayout("recordCarrierHandover", {}, de ? "Carrier-Uebergabe erfasst" : "Carrier handover recorded")} type="button">{de ? "Uebergabe" : "Handover"}</button>
            </div>
            <div className="warehouse-source-match-list">
              {snapshot.wavePlans.slice(-2).reverse().map((wave) => (
                <a className="warehouse-source-match" href={panelHref("wave", wave.id, "right")} key={wave.id}>
                  <span><strong>{wave.id}</strong><small>{wave.status} · {wave.priority} · {wave.lines.length} {de ? "Auftraege" : "orders"}</small></span>
                </a>
              ))}
              {transferRows.map((transfer) => (
                <a className="warehouse-source-match" href={panelHref("transfer", transfer.id, "right")} key={transfer.id}>
                  <span><strong>{transfer.id}</strong><small>{transfer.status} · {locationName(transfer.fromLocationId)} {"->"} {locationName(transfer.toLocationId)}</small></span>
                </a>
              ))}
              {labelledPackages.slice(0, 2).map((pkg) => (
                <a className="warehouse-source-match" href={panelHref("package", pkg.id, "right")} key={pkg.id}>
                  <span><strong>{pkg.id}</strong><small>{pkg.carrier ?? "Carrier"} · {pkg.trackingNumber}</small></span>
                </a>
              ))}
            </div>
          </section>
          <section className="warehouse-match-panel warehouse-cart-panel warehouse-ops-panel">
            <h3>{de ? "Retoure, Sync, Abnahme" : "Returns, sync, acceptance"}</h3>
            <div className="warehouse-ops-actions">
              <button disabled={busy !== null || returnableShipments.length === 0} onClick={() => void runLayout("authorizeReturn", {}, de ? "Retoure autorisiert" : "Return authorized")} type="button">{de ? "Retoure" : "Return"}</button>
              <button disabled={busy !== null || authorizedReturns.length === 0} onClick={() => void runLayout("receiveReturn", {}, de ? "Retoure eingebucht" : "Return received")} type="button">{de ? "Retoure rein" : "Receive return"}</button>
              <button disabled={busy !== null} onClick={() => void runLayout("recordImportDryRun", {}, de ? "Import-Testlauf gespeichert" : "Import dry run saved")} type="button">{de ? "Import pruefen" : "Check import"}</button>
              <button disabled={busy !== null} onClick={() => void runLayout("recordSyncConflict", {}, de ? "Sync-Konflikt erfasst" : "Sync conflict recorded")} type="button">{de ? "Sync pruefen" : "Check sync"}</button>
              <button disabled={busy !== null} onClick={() => void runLayout("recordOpsHandover", {}, de ? "Schichtuebergabe erfasst" : "Handover recorded")} type="button">{de ? "Schicht" : "Shift"}</button>
              <button disabled={busy !== null} onClick={() => void runLayout("recordRoleReview", {}, de ? "Rechtepruefung erfasst" : "Role review recorded")} type="button">{de ? "Rechte" : "Roles"}</button>
              <button disabled={busy !== null} onClick={() => void runLayout("recordThreePlCharge", {}, de ? "3PL-Leistung gebucht" : "3PL charge recorded")} type="button">3PL</button>
              <a href="/api/business/warehouse?format=csv">{de ? "Report CSV" : "Report CSV"}</a>
            </div>
            <div className="warehouse-source-match-list">
              {lowStockRows.map(({ available, item }) => (
                <div className="warehouse-source-match" key={item.id}>
                  <span><strong>{item.sku}</strong><small>{de ? "Mindestbestand" : "Low stock"} · {available}/5 · {item.name}</small></span>
                </div>
              ))}
              {riskRows.map((risk) => (
                <div className="warehouse-source-match" key={risk.id}>
                  <span><strong>{risk.type}</strong><small>{risk.label}</small></span>
                </div>
              ))}
              {snapshot.integrationEvents.slice(-3).reverse().map((event) => (
                <a className="warehouse-source-match" href={panelHref("integration-event", event.id, "right")} key={event.id}>
                  <span><strong>{event.eventType}</strong><small>{event.provider} · {event.source}</small></span>
                </a>
              ))}
            </div>
          </section>
        </div>
      </section>

      <aside className={`warehouse-bottom-module ${bottomOpen ? "is-open" : ""}`} aria-label={de ? "Arbeitsmodul" : "Work module"}>
        <button className="warehouse-bottom-tab" onClick={() => setBottomOpen(!bottomOpen)} type="button">
          <span>{focusType === "location" && selectedStructureLocation ? selectedStructureLocation.name : focusType === "item" && selectedItem ? selectedItem.name : selectedSlot ? selectedSlot.name : de ? "Arbeitsmodul" : "Work module"}</span>
          <strong>{bottomOpen ? (de ? "Zuklappen" : "Close") : (de ? "Aufklappen" : "Open")}</strong>
        </button>
        {bottomOpen ? (
          <div className="warehouse-bottom-body">
            <nav className="warehouse-bottom-tabs" aria-label={de ? "Arbeitsmodus" : "Work mode"}>
              {[
                ["location", de ? "Struktur" : "Structure"],
                ["item", de ? "Artikel" : "Item"],
                ["slot", de ? "Slot editieren" : "Edit slot"],
                ["stock", de ? "Bestand" : "Stock"],
                ["count", de ? "Inventur" : "Count"],
                ["audit", de ? "Audit" : "Audit"]
              ].map(([id, label]) => (
                <button className={mode === id ? "is-active" : ""} key={id} onClick={() => setMode(id as typeof mode)} type="button">{label}</button>
              ))}
            </nav>
            {focusType === "location" && selectedStructureLocation ? (
              <div className="warehouse-bottom-grid">
                <form
                  key={`location-${selectedStructureLocation.id}`}
                  className="warehouse-bottom-editor"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    void runLayout("renameLocation", {
                      locationName: String(form.get("locationName") ?? selectedStructureLocation.name),
                      parentId: selectedStructureLocation.id
                    }, de ? "Struktur gespeichert" : "Structure saved");
                  }}
                >
                  <label>
                    <span>{de ? "Name" : "Name"}</span>
                    <input name="locationName" defaultValue={selectedStructureLocation.name} />
                  </label>
                  <label>
                    <span>{de ? "Ebene" : "Level"}</span>
                    <input readOnly value={selectedStructureLocation.kind} />
                  </label>
                  <label>
                    <span>{de ? "Status" : "Status"}</span>
                    <input readOnly value={inactiveLocationIds.has(selectedStructureLocation.id) ? (de ? "inaktiv" : "inactive") : (de ? "aktiv" : "active")} />
                  </label>
                  <label>
                    <span>{de ? "Neue Slots" : "New slots"}</span>
                    <input
                      min="1"
                      max="24"
                      name="slotCount"
                      type="number"
                      value={structureSlotCount}
                      onChange={(event) => setStructureSlotCount(Number(event.target.value) || 1)}
                    />
                  </label>
                  <button disabled={busy !== null} type="submit">{de ? "Speichern" : "Save"}</button>
                  <button disabled={busy !== null || selectedStructureLocation.kind === "bin"} onClick={() => void runLayout(selectedStructureLocation.kind === "warehouse" ? "createSection" : "createSlot", {
                    parentId: selectedStructureLocation.id,
                    slotCount: structureSlotCount
                  }, selectedStructureLocation.kind === "warehouse" ? (de ? "Bereich angelegt" : "Section added") : (de ? "Slots angelegt" : "Slots added"))} type="button">
                    {selectedStructureLocation.kind === "warehouse" ? (de ? "Bereich" : "Section") : (de ? "Slots" : "Slots")}
                  </button>
                  <button disabled={busy !== null} onClick={() => void runLayout("duplicateLocation", { parentId: selectedStructureLocation.id }, de ? "Struktur dupliziert" : "Structure duplicated")} type="button">
                    {de ? "Duplizieren" : "Duplicate"}
                  </button>
                  <button disabled={busy !== null} onClick={() => void runLayout("toggleLocationActive", { parentId: selectedStructureLocation.id }, inactiveLocationIds.has(selectedStructureLocation.id) ? (de ? "Struktur aktiviert" : "Structure activated") : (de ? "Struktur deaktiviert" : "Structure deactivated"))} type="button">
                    {inactiveLocationIds.has(selectedStructureLocation.id) ? (de ? "Aktivieren" : "Activate") : (de ? "Deaktivieren" : "Deactivate")}
                  </button>
                </form>
                <div className="warehouse-bottom-stock">
                  <a href={panelHref("location", selectedStructureLocation.id, "right")}>
                    <span><strong>{de ? "Auswirkung" : "Impact"}</strong><small>{descendantIds(selectedStructureLocation.id).length} {de ? "untergeordnete Plaetze" : "child locations"}</small></span>
                    <em>{snapshot.balances.filter((balance) => new Set([selectedStructureLocation.id, ...descendantIds(selectedStructureLocation.id)]).has(balance.locationId)).reduce((sum, balance) => sum + balance.quantity, 0)}</em>
                  </a>
                  {childLocations(selectedStructureLocation.id).slice(0, 6).map((location) => (
                    <button className="warehouse-flow-item" key={location.id} onClick={() => selectStructureLocation(location.id)} type="button">
                      <span><strong>{location.name}</strong><small>{location.kind}</small></span>
                    </button>
                  ))}
                </div>
                <div className="warehouse-bottom-ai">
                  <strong>{de ? "Strukturvorschau" : "Structure preview"}</strong>
                  {selectedStructureLocation.kind === "warehouse" ? (
                    <span>
                      <b>{de ? "Naechster Bereich" : "Next section"}</b>
                      <small>{previewSectionCode(selectedStructureLocation.id)}-Section · {selectedStructureLocation.name}</small>
                    </span>
                  ) : selectedStructureLocation.kind === "zone" ? (
                    <div className="warehouse-structure-preview">
                      {previewSlots(selectedStructureLocation, structureSlotCount).map((slot) => (
                        <span key={slot.name}>
                          <b>{slot.name}</b>
                          <small>{slot.slotType} · {de ? "Regal" : "bay"} {slot.bay} · L{slot.level} · {slot.capacity}</small>
                        </span>
                      ))}
                    </div>
                  ) : (
                    <span>{de ? "Slots werden einzeln bearbeitet." : "Slots are edited one by one."}</span>
                  )}
                  <span>{de ? "Deaktivieren nimmt die Struktur aus neuen Lageraktionen, Historie und Bestaende bleiben sichtbar." : "Deactivation removes the structure from new work while history and stock stay visible."}</span>
                  <span>{de ? "Rechtsklick auf Lager, Zone oder Slot oeffnet CTOX-Strukturaktionen." : "Right-click warehouse, zone, or slot for CTOX structure actions."}</span>
                </div>
              </div>
            ) : focusType === "item" && selectedItem ? (
              <div className="warehouse-bottom-grid">
                <form
                  key={`item-${selectedItem.id}`}
                  className="warehouse-bottom-editor"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    void runLayout("renameItem", {
                      inventoryItemId: selectedItem.id,
                      itemName: String(form.get("itemName") ?? selectedItem.name),
                      itemSku: String(form.get("itemSku") ?? selectedItem.sku),
                      itemTrackingMode: String(form.get("itemTrackingMode") ?? selectedItem.trackingMode),
                      itemUom: String(form.get("itemUom") ?? selectedItem.uom)
                    }, de ? "Artikel gespeichert" : "Item saved");
                  }}
                >
                  <label>
                    <span>{de ? "Name" : "Name"}</span>
                    <input name="itemName" defaultValue={selectedItem.name} />
                  </label>
                  <label>
                    <span>SKU</span>
                    <input name="itemSku" defaultValue={selectedItem.sku} />
                  </label>
                  <label>
                    <span>{de ? "Einheit" : "Unit"}</span>
                    <input name="itemUom" defaultValue={selectedItem.uom} />
                  </label>
                  <label>
                    <span>Tracking</span>
                    <select name="itemTrackingMode" defaultValue={selectedItem.trackingMode}>
                      <option value="none">{de ? "ohne" : "none"}</option>
                      <option value="lot">{de ? "Charge" : "lot"}</option>
                      <option value="serial">{de ? "Seriennummer" : "serial"}</option>
                    </select>
                  </label>
                  <button disabled={busy !== null} type="submit">{de ? "Speichern" : "Save"}</button>
                  <button disabled={busy !== null} onClick={() => void runLayout("duplicateItem", { inventoryItemId: selectedItem.id }, de ? "Artikel dupliziert" : "Item duplicated")} type="button">
                    {de ? "Duplizieren" : "Duplicate"}
                  </button>
                  <button disabled={busy !== null || inactiveItemIds.has(selectedItem.id)} onClick={() => void runLayout("deactivateItem", { inventoryItemId: selectedItem.id }, de ? "Artikel deaktiviert" : "Item deactivated")} type="button">
                    {de ? "Deaktivieren" : "Deactivate"}
                  </button>
                </form>
                <div className="warehouse-bottom-stock">
                  {snapshot.balances.filter((balance) => balance.inventoryItemId === selectedItem.id && balance.quantity > 0).slice(0, 5).map((balance) => (
                    <a href={panelHref("balance", balance.balanceKey, "right")} key={balance.balanceKey}>
                      <span><strong>{locationName(balance.locationId)}</strong><small>{ownerName(balance.inventoryOwnerPartyId)} · {balance.stockStatus}</small></span>
                      <em>{balance.quantity}</em>
                    </a>
                  ))}
                </div>
                <div className="warehouse-bottom-ai">
                  <strong>{de ? "Artikel-Gates" : "Item gates"}</strong>
                  <span>{inactiveItemIds.has(selectedItem.id) ? (de ? "Artikel ist fuer neue Vorgaenge deaktiviert." : "Item is inactive for new work.") : (de ? "Artikel ist aktiv." : "Item is active.")}</span>
                  <span>{de ? "Tracking, Owner und Lagerbedingungen muessen vor Wareneingang eindeutig sein." : "Tracking, owner, and storage rules must be clear before receiving."}</span>
                </div>
              </div>
            ) : selectedSlot ? (
              mode === "audit" ? (
                <div className="warehouse-bottom-grid warehouse-audit-grid">
                  <div className="warehouse-bottom-stock warehouse-audit-list">
                    {selectedAuditMovements.map((movement) => (
                      <a href={panelHref("movement", movement.id, "right")} key={movement.id}>
                        <span>
                          <strong>{movement.movementType} · {itemName(movement.inventoryItemId)}</strong>
                          <small>{ownerName(movement.inventoryOwnerPartyId)} · {movement.stockStatus} · {movement.sourceType}:{movement.sourceId}</small>
                        </span>
                        <em>{movement.quantity}</em>
                      </a>
                    ))}
                    {selectedAuditMovements.length === 0 ? <span className="warehouse-empty-note">{de ? "Noch keine Bewegungen fuer diesen Slot." : "No movements for this slot yet."}</span> : null}
                  </div>
                  <div className="warehouse-bottom-stock">
                    {snapshot.commandLog.filter((command) => command.refId === selectedSlot.id || command.refType === "stock_balance").slice(-6).reverse().map((command) => (
                      <a href={panelHref("command", command.idempotencyKey, "right")} key={command.idempotencyKey}>
                        <span><strong>{command.type}</strong><small>{command.refType} · {command.requestedBy}</small></span>
                        <em>{command.requestedAt.slice(11, 16)}</em>
                      </a>
                    ))}
                  </div>
                  <div className="warehouse-bottom-ai">
                    <strong>{de ? "Audit-Gate" : "Audit gate"}</strong>
                    <span>{de ? "Bewegungen und Commands bleiben getrennt sichtbar: physischer Bestand, Korrektur, Reservierung und Strukturaktion." : "Movements and commands stay separately visible: physical stock, correction, reservation, and structure action."}</span>
                    <a href={panelHref("location", selectedSlot.id, "right")}>{de ? "Audit rechts oeffnen" : "Open audit right"}</a>
                  </div>
                </div>
              ) : mode === "count" ? (
                <div className="warehouse-bottom-grid warehouse-count-grid">
                  <form
                    key={`count-${selectedOpenCycleCount?.id ?? selectedSlot.id}`}
                    className="warehouse-bottom-editor warehouse-count-editor"
                    onSubmit={(event) => {
                      event.preventDefault();
                      if (!selectedOpenCycleCount) return;
                      const form = new FormData(event.currentTarget);
                      const countedQuantities = Object.fromEntries(selectedOpenCycleCount.lines.map((line) => {
                        const value = Number(form.get(`counted-${line.id}`));
                        return [line.id, Number.isFinite(value) ? value : line.expectedQuantity];
                      }));
                      void runLayout("recordCycleCount", {
                        countId: selectedOpenCycleCount.id,
                        countedQuantities
                      }, de ? "Zaehlung gespeichert" : "Count saved");
                    }}
                  >
                    <label>
                      <span>{de ? "Inventur" : "Cycle count"}</span>
                      <input readOnly value={selectedOpenCycleCount?.id ?? (de ? "Keine offene Zaehlung" : "No open count")} />
                    </label>
                    <label>
                      <span>{de ? "Status" : "Status"}</span>
                      <input readOnly value={selectedOpenCycleCount?.status ?? selectedLatestCycleCount?.status ?? "-"} />
                    </label>
                    {selectedOpenCycleCount ? selectedOpenCycleCount.lines.map((line) => (
                      <label className="warehouse-count-line" key={line.id}>
                        <span>{itemName(line.inventoryItemId)} · Soll {line.expectedQuantity}</span>
                        <input min="0" name={`counted-${line.id}`} type="number" defaultValue={line.countedQuantity ?? line.expectedQuantity} />
                      </label>
                    )) : null}
                    <button disabled={busy !== null || !selectedSlot || Boolean(selectedOpenCycleCount)} onClick={() => void runLayout("openCycleCount", { parentId: selectedSlot.id }, de ? "Inventur gestartet" : "Count opened")} type="button">
                      {de ? "Zaehlung starten" : "Start count"}
                    </button>
                    <button disabled={busy !== null || !selectedOpenCycleCount} type="submit">{de ? "Zaehlung speichern" : "Save count"}</button>
                    <button disabled={busy !== null || !selectedOpenCycleCount} onClick={() => selectedOpenCycleCount ? void runLayout("closeCycleCount", { countId: selectedOpenCycleCount.id }, de ? "Differenz gebucht" : "Variance posted") : undefined} type="button">
                      {de ? "Differenz buchen" : "Post variance"}
                    </button>
                  </form>
                  <div className="warehouse-bottom-stock warehouse-count-stock">
                    {(selectedOpenCycleCount?.lines ?? selectedLatestCycleCount?.lines ?? []).map((line) => (
                      <a href={panelHref("cycle-count", line.id, "right")} key={line.id}>
                        <span><strong>{itemName(line.inventoryItemId)}</strong><small>{line.stockStatus} · Ist {line.countedQuantity ?? "-"} · Delta {line.varianceQuantity ?? 0}</small></span>
                        <em>{line.expectedQuantity}</em>
                      </a>
                    ))}
                    {!selectedOpenCycleCount && !selectedLatestCycleCount ? <span className="warehouse-empty-note">{de ? "Noch keine Inventur fuer diesen Slot." : "No count for this slot yet."}</span> : null}
                  </div>
                  <div className="warehouse-bottom-ai">
                    <strong>{de ? "Inventur-Gate" : "Count gate"}</strong>
                    <span>{de ? "Erst zaehlen, dann Differenz buchen. Der Abschluss erzeugt Adjustments und Bewegungen." : "Count first, then post variance. Closing creates adjustments and movements."}</span>
                    <a href={panelHref("location", selectedSlot.id, "right")}>{de ? "Bewegungen rechts pruefen" : "Check movements right"}</a>
                  </div>
                </div>
              ) : (
              <div className="warehouse-bottom-grid">
                {mode === "stock" ? (
                  <div className="warehouse-stock-forms">
                    <form
                      key={`move-${pendingMove?.balanceKey ?? selectedSlot.id}-${pendingMove?.targetLocationId ?? selectedSlot.id}`}
                      className="warehouse-bottom-editor warehouse-move-editor"
                      onSubmit={(event) => {
                        event.preventDefault();
                        const form = new FormData(event.currentTarget);
                        const balanceKey = String(form.get("balanceKey") ?? "");
                        const targetLocationId = String(form.get("targetLocationId") ?? selectedSlot.id);
                        const quantity = Number(form.get("quantity"));
                        void runLayout("moveStock", {
                          balanceKey,
                          quantity: Number.isFinite(quantity) ? quantity : undefined,
                          targetLocationId
                        }, de ? "Bestand verschoben" : "Stock moved").then(() => setPendingMove(undefined));
                      }}
                    >
                      <label>
                        <span>{de ? "Quelle" : "Source"}</span>
                        <select name="balanceKey" defaultValue={pendingMove?.balanceKey ?? selectedSlotBalances[0]?.balanceKey ?? stockRows[0]?.balanceKey ?? ""}>
                          {[...selectedSlotBalances, ...stockRows.filter((balance) => !selectedSlotBalances.some((entry) => entry.balanceKey === balance.balanceKey))].map((balance) => (
                            <option key={balance.balanceKey} value={balance.balanceKey}>
                              {ownerName(balance.inventoryOwnerPartyId)} · {itemName(balance.inventoryItemId)} · {locationName(balance.locationId)} · {balance.quantity}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        <span>{de ? "Zielslot" : "Target slot"}</span>
                        <select name="targetLocationId" defaultValue={pendingMove?.targetLocationId ?? selectedSlot.id}>
                          {warehouseSlots.map((slot) => (
                            <option key={slot.id} value={slot.id}>{slot.name} · {childLocations(slot.parentId ?? "").find((location) => location.id === slot.parentId)?.name ?? locationName(slot.parentId ?? "")}</option>
                          ))}
                        </select>
                      </label>
                      <label>
                        <span>{de ? "Menge" : "Quantity"}</span>
                        <input min="1" name="quantity" type="number" defaultValue={selectedMoveBalance?.quantity ?? selectedSlotBalances[0]?.quantity ?? 1} />
                      </label>
                      <label>
                        <span>{de ? "Pruefung" : "Check"}</span>
                        <input readOnly value={selectedMoveBalance ? `${ownerName(selectedMoveBalance.inventoryOwnerPartyId)} · ${locationName(selectedMoveBalance.locationId)} -> ${locationName(pendingMove?.targetLocationId ?? selectedSlot.id)}` : (de ? "Quelle auswaehlen" : "Select source")} />
                      </label>
                      <button disabled={busy !== null || stockRows.length === 0} type="submit">{de ? "Umlagern" : "Move stock"}</button>
                      <button disabled={busy !== null || !pendingMove} onClick={() => setPendingMove(undefined)} type="button">{de ? "Verwerfen" : "Discard"}</button>
                    </form>
                    <form
                      className="warehouse-bottom-editor warehouse-correction-editor"
                      onSubmit={(event) => {
                        event.preventDefault();
                        const form = new FormData(event.currentTarget);
                        const balanceKey = String(form.get("balanceKey") ?? "");
                        const quantity = Number(form.get("quantity"));
                        const adjustedQuantity = Number(form.get("adjustedQuantity"));
                        const stockStatusTo = String(form.get("stockStatusTo") ?? "quarantine");
                        const reasonCode = String(form.get("reasonCode") ?? "manual");
                        const correctionAction = String(form.get("correctionAction") ?? "status");
                        if (correctionAction === "adjust") {
                          void runLayout("adjustBalance", {
                            adjustedQuantity: Number.isFinite(adjustedQuantity) ? adjustedQuantity : undefined,
                            balanceKey,
                            reasonCode
                          }, de ? "Bestand korrigiert" : "Stock adjusted");
                        } else {
                          void runLayout("changeStockStatus", {
                            balanceKey,
                            quantity: Number.isFinite(quantity) ? quantity : undefined,
                            reasonCode,
                            stockStatusTo
                          }, stockStatusTo === "available" ? (de ? "Bestand freigegeben" : "Stock released") : (de ? "Bestand gesperrt" : "Stock blocked"));
                        }
                      }}
                    >
                      <label>
                        <span>{de ? "Bestand" : "Balance"}</span>
                        <select name="balanceKey" defaultValue={selectedSlotBalances.find((balance) => balance.stockStatus !== "shipped")?.balanceKey ?? selectedSlotBalances[0]?.balanceKey ?? ""}>
                          {selectedSlotBalances.filter((balance) => balance.stockStatus !== "shipped").map((balance) => (
                            <option key={balance.balanceKey} value={balance.balanceKey}>
                              {ownerName(balance.inventoryOwnerPartyId)} · {itemName(balance.inventoryItemId)} · {balance.stockStatus} · {balance.quantity}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        <span>{de ? "Aktion" : "Action"}</span>
                        <select name="correctionAction" defaultValue="status">
                          <option value="status">{de ? "Status wechseln" : "Change status"}</option>
                          <option value="adjust">{de ? "Menge korrigieren" : "Adjust quantity"}</option>
                        </select>
                      </label>
                      <label>
                        <span>{de ? "Zielstatus" : "Target status"}</span>
                        <select name="stockStatusTo" defaultValue="quarantine">
                          <option value="quarantine">{de ? "QS/Sperrbestand" : "Quarantine"}</option>
                          <option value="damaged">{de ? "Beschaedigt" : "Damaged"}</option>
                          <option value="available">{de ? "Freigeben" : "Available"}</option>
                        </select>
                      </label>
                      <label>
                        <span>{de ? "Menge" : "Quantity"}</span>
                        <input min="1" name="quantity" type="number" defaultValue={1} />
                      </label>
                      <label>
                        <span>{de ? "Korrektur auf" : "Adjust to"}</span>
                        <input min="0" name="adjustedQuantity" type="number" defaultValue={selectedSlotBalances.find((balance) => balance.stockStatus !== "shipped")?.quantity ?? 0} />
                      </label>
                      <label>
                        <span>{de ? "Grundcode" : "Reason"}</span>
                        <select name="reasonCode" defaultValue="qa_hold">
                          <option value="qa_hold">{de ? "QS-Pruefung" : "QA hold"}</option>
                          <option value="damage">{de ? "Schaden" : "Damage"}</option>
                          <option value="count_fix">{de ? "Inventurkorrektur" : "Count fix"}</option>
                          <option value="manual_release">{de ? "Freigabe" : "Manual release"}</option>
                        </select>
                      </label>
                      <button disabled={busy !== null || selectedSlotBalances.filter((balance) => balance.stockStatus !== "shipped").length === 0} type="submit">
                        {de ? "Buchen" : "Post"}
                      </button>
                    </form>
                  </div>
                ) : (
                  <form
                    key={`slot-${selectedSlot.id}`}
                    className="warehouse-bottom-editor"
                    onSubmit={(event) => {
                      event.preventDefault();
                      const form = new FormData(event.currentTarget);
                      void runLayout("renameLocation", {
                        locationAisle: String(form.get("locationAisle") ?? ""),
                        locationBay: String(form.get("locationBay") ?? ""),
                        locationCapacityUnits: Number(form.get("locationCapacityUnits")),
                        locationLevel: String(form.get("locationLevel") ?? ""),
                        locationName: String(form.get("locationName") ?? selectedSlot.name),
                        locationPositionNote: String(form.get("locationPositionNote") ?? ""),
                        locationSlotType: String(form.get("locationSlotType") ?? selectedSlot.slotType ?? "standard"),
                        parentId: selectedSlot.id
                      }, de ? "Slot gespeichert" : "Slot saved");
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
                    <label>
                      <span>{de ? "Kapazitaet" : "Capacity"}</span>
                      <input min="0" name="locationCapacityUnits" type="number" defaultValue={selectedSlot.capacityUnits ?? 100} />
                    </label>
                    <label>
                      <span>{de ? "Slot-Typ" : "Slot type"}</span>
                      <select name="locationSlotType" defaultValue={selectedSlot.slotType ?? "standard"}>
                        <option value="standard">{de ? "Standard" : "Standard"}</option>
                        <option value="pick_face">{de ? "Pickfach" : "Pick face"}</option>
                        <option value="bulk">{de ? "Bulk" : "Bulk"}</option>
                        <option value="staging">{de ? "Bereitstellung" : "Staging"}</option>
                        <option value="quarantine">{de ? "QS/Sperre" : "Quarantine"}</option>
                        <option value="returns">{de ? "Retoure" : "Returns"}</option>
                      </select>
                    </label>
                    <label>
                      <span>{de ? "Gang" : "Aisle"}</span>
                      <input name="locationAisle" defaultValue={selectedSlot.aisle ?? ""} />
                    </label>
                    <label>
                      <span>{de ? "Regal" : "Bay"}</span>
                      <input name="locationBay" defaultValue={selectedSlot.bay ?? ""} />
                    </label>
                    <label>
                      <span>{de ? "Ebene" : "Level"}</span>
                      <input name="locationLevel" defaultValue={selectedSlot.level ?? ""} />
                    </label>
                    <label>
                      <span>{de ? "Hinweis" : "Note"}</span>
                      <input name="locationPositionNote" defaultValue={selectedSlot.positionNote ?? ""} />
                    </label>
                    <button disabled={busy !== null} type="submit">{de ? "Speichern" : "Save"}</button>
                    <button disabled={busy !== null} onClick={() => void runLayout("toggleLocationPickable", { parentId: selectedSlot.id }, de ? "Status geaendert" : "Status changed")} type="button">
                      {selectedSlot.pickable ? (de ? "Sperren" : "Block") : (de ? "Entsperren" : "Unblock")}
                    </button>
                    <button disabled={busy !== null} onClick={() => void runLayout("duplicateLocation", { parentId: selectedSlot.id }, de ? "Slot dupliziert" : "Slot duplicated")} type="button">
                      {de ? "Duplizieren" : "Duplicate"}
                    </button>
                  </form>
                )}
                <div className="warehouse-bottom-stock">
                  {(mode === "slot" ? selectedSlotBalances.slice(0, 3) : selectedSlotBalances).map((balance) => (
                    <a href={panelHref("balance", balance.balanceKey, "right")} key={balance.balanceKey}>
                      <span><strong>{itemName(balance.inventoryItemId)}</strong><small>{ownerName(balance.inventoryOwnerPartyId)} · {balance.stockStatus}</small></span>
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
              )
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
