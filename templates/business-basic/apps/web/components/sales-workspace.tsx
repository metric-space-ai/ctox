import { resolveLocale, type WorkSurfacePanelState } from "@ctox-business/ui";
import type { ReactNode } from "react";
import {
  getSalesBundle,
  text,
  type SalesBundle,
  type SalesAccount,
  type SalesContact,
  type SalesCustomer,
  type SalesLead,
  type SalesOffer,
  type SalesOpportunity,
  type SalesOpportunityStage,
  type SalesTask,
  type SupportedLocale
} from "../lib/sales-seed";
import { InvoiceCustomerEditor, type InvoiceCustomerOption } from "./invoice-customer-editor";
import { InvoiceLinesEditor, type InvoiceLineDraft } from "./invoice-lines-editor";
import { LexicalRichTextEditor } from "./lexical-rich-text-editor";
import { SalesPipelineView } from "./sales/sales-pipeline";
import { LeadFlowCanvas, type LeadFlow, type LeadFlowLink, type LeadFlowNode, type LeadFlowNodeState } from "./sales/lead-flow-canvas";
import { SalesCreateForm, SalesQueueButton } from "./sales/actions";
import { SalesCampaignsView } from "./sales/sales-campaigns";

type QueryState = {
  locale?: string;
  theme?: string;
  panel?: string;
  recordId?: string;
  drawer?: string;
  mode?: string;
  search?: string;
  selectedId?: string;
  stage?: string;
};

const pipelineStages: SalesOpportunityStage[] = ["Qualify", "Discover", "Proposal", "Negotiation", "Won"];
const leadActivityStages = ["Produktdemo", "Produkttest", "Meeting", "Schriftverkehr", "Angebot"] as const;

export async function SalesWorkspace({
  submoduleId,
  query
}: {
  submoduleId: string;
  query: QueryState;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = salesCopy[locale];
  const data = await getSalesBundle();

  if (submoduleId === "campaigns") return <SalesCampaignsView data={data} locale={locale} query={query} />;
  if (submoduleId === "accounts" || submoduleId === "leads") return <LeadAccountsView copy={copy} data={data} locale={locale} query={query} submoduleId="leads" />;
  if (submoduleId === "contacts") return <ContactsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "offers") return <OffersView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "customers") return <CustomersView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "tasks") return <TasksView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;

  return <SalesPipelineView query={query} />;
}

export async function SalesPanel({
  panelState,
  query,
  submoduleId
}: {
  panelState?: WorkSurfacePanelState;
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = salesCopy[locale];
  const data = await getSalesBundle();
  const panel = panelState?.panel;
  const recordId = panelState?.recordId;

  if (panel === "sales-set") {
    const salesSet = resolveSalesSet(recordId, data, copy);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={salesSet.title} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{salesSet.description}</p>
        <dl className="drawer-facts">
          <Fact label={copy.items} value={String(salesSet.items.length)} />
          <Fact label={copy.value} value={salesSet.value ? formatMoney(salesSet.value) : "-"} />
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.selectedItems}</h3>
          <div className="ops-mini-list">
            {salesSet.items.map((item) => (
              <a
                data-context-item
                data-context-label={item.label}
                data-context-module="sales"
                data-context-record-id={item.id}
                data-context-record-type={item.type}
                data-context-submodule={submoduleId}
                href={salesRecordHref(query, submoduleId, item.panel, item.id)}
                key={`${item.type}-${item.id}`}
              >
                {item.label} · {item.meta}
              </a>
            ))}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.sync}</h3>
          <SalesQueueButton
            action="sync"
            className="drawer-primary"
            instruction={`Review and synchronize this Sales context set: ${salesSet.title}.`}
            payload={{ filter: recordId, items: salesSet.items }}
            recordId={recordId ?? "sales-set"}
            resource={salesSet.resource}
            title={`Sync Sales set: ${salesSet.title}`}
          >
            {copy.askCtoxSet}
          </SalesQueueButton>
        </section>
      </div>
    );
  }

  if (panel === "new") {
    const resource = resolveNewResource(recordId, submoduleId);
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={copy.newRecord} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{copy.newRecordDescription}</p>
        <SalesCreateForm
          accounts={data.accounts.map((account) => ({ label: account.name, value: account.id }))}
          contacts={data.contacts.map((contact) => ({ label: contact.name, value: contact.id }))}
          owners={data.owners.map((owner) => ({ label: owner.name, value: owner.id }))}
          queueLabel={copy.queueCreate}
          resource={resource}
        />
      </div>
    );
  }

  const campaign = data.campaigns.find((item) => item.id === recordId);
  if (panel === "campaign" && campaign) {
    const owner = data.owners.find((item) => item.id === campaign.ownerId);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={campaign.name} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(campaign.nextStep, locale)}</p>
        <dl className="drawer-facts">
          <Fact label="Status" value={campaign.status} />
          <Fact label={copy.owner} value={owner?.name} />
          <Fact label="Importe" value={String(campaign.importedRecords)} />
          <Fact label="Research" value={String(campaign.enrichedRecords)} />
          <Fact label="Zugeordnet" value={String(campaign.assignedRecords)} />
          <Fact label="Quellen" value={campaign.sourceTypes.join(", ")} />
        </dl>
        <section className="ops-drawer-section">
          <h3>Zuordnungs-Prompt</h3>
          <p>{text(campaign.assignmentPrompt, locale)}</p>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.sync}</h3>
          <SalesQueueButton
            action="sync"
            className="drawer-primary"
            instruction={`Review Sales campaign ${campaign.name}, refresh research enrichment, and update prompt-based record assignment.`}
            payload={{ campaign }}
            recordId={campaign.id}
            resource="campaigns"
            title={`Sync campaign: ${campaign.name}`}
          >
            {copy.askCtox}
          </SalesQueueButton>
        </section>
      </div>
    );
  }

  const opportunity = data.opportunities.find((item) => item.id === recordId);
  if (panel === "opportunity" && opportunity) {
    const account = data.accounts.find((item) => item.id === opportunity.accountId);
    const contact = data.contacts.find((item) => item.id === opportunity.contactId);
    const owner = data.owners.find((item) => item.id === opportunity.ownerId);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={opportunity.name} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(opportunity.nextStep, locale)}</p>
        <dl className="drawer-facts">
          <Fact label={copy.account} value={account?.name} />
          <Fact label={copy.contact} value={contact?.name} />
          <Fact label={copy.stage} value={opportunity.stage} />
          <Fact label={copy.value} value={formatMoney(opportunity.value)} />
          <Fact label={copy.probability} value={`${opportunity.probability}%`} />
          <Fact label={copy.closeDate} value={opportunity.closeDate} />
          <Fact label={copy.owner} value={owner?.name} />
          <Fact label={copy.source} value={opportunity.source} />
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.risks}</h3>
          <div className="ops-mini-list">
            {opportunity.risks.length > 0 ? opportunity.risks.map((risk) => (
              <span key={text(risk, locale)}>{text(risk, locale)}</span>
            )) : <span>{copy.noRisks}</span>}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.sync}</h3>
          <SalesQueueButton
            action="sync"
            className="drawer-primary"
            instruction={`Synchronize Sales opportunity ${opportunity.name} with CRM context, Operations handoff, Business forecast, and CTOX queue.`}
            payload={{ opportunity, account, contact }}
            recordId={opportunity.id}
            resource="opportunities"
            title={`Sync opportunity: ${opportunity.name}`}
          >
            {copy.askCtox}
          </SalesQueueButton>
        </section>
      </div>
    );
  }

  const offer = data.offers.find((item) => item.id === recordId);
  if (panel === "offer" && offer) {
    const account = data.accounts.find((item) => item.id === offer.accountId);
    const contact = data.contacts.find((item) => item.id === offer.contactId);
    const opportunityForOffer = data.opportunities.find((item) => item.id === offer.opportunityId);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={`${offer.number} · ${offer.title}`} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(offer.nextStep, locale)}</p>
        <dl className="drawer-facts">
          <Fact label={copy.account} value={account?.name} />
          <Fact label={copy.contact} value={contact?.name} />
          <Fact label={copy.opportunity} value={opportunityForOffer?.name} />
          <Fact label={copy.status} value={offer.status} />
          <Fact label={copy.validUntil} value={offer.validUntil} />
          <Fact label={copy.net} value={formatMoney(offer.netAmount, offer.currency, locale)} />
          <Fact label={copy.gross} value={formatMoney(offer.grossAmount, offer.currency, locale)} />
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.offerText}</h3>
          <div className="ops-mini-list">
            <span>{text(offer.introText, locale)}</span>
            <span>{text(offer.paymentTerms, locale)}</span>
            <span>{text(offer.closingText, locale)}</span>
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.lines}</h3>
          <div className="ops-mini-list">
            {offer.lineItems.map((line) => (
              <span key={`${offer.id}-${line.description}`}>
                {line.description} · {line.quantity} {line.unit} · {formatMoney(line.unitPrice, offer.currency, locale)} · {line.taxRate}%
              </span>
            ))}
          </div>
        </section>
        <div className="ops-action-dock">
          <SalesQueueButton
            action="sync"
            className="drawer-primary"
            instruction={`Synchronize Sales offer ${offer.number} with lead activity history, document status, customer handoff readiness, and Operations onboarding context.`}
            payload={{ offer, account, contact, opportunity: opportunityForOffer }}
            recordId={offer.id}
            resource="offers"
            title={`Sync offer: ${offer.number}`}
          >
            {copy.askCtox}
          </SalesQueueButton>
          <SalesQueueButton
            action="convert"
            instruction={`Convert accepted Sales offer ${offer.number} into a Sales customer record and queue an Operations onboarding project. Do not create a Business invoice until the customer handoff is confirmed.`}
            payload={{ offer, account, contact, opportunity: opportunityForOffer }}
            recordId={offer.id}
            resource="customers"
            title={`Create customer from accepted offer: ${offer.number}`}
          >
            {copy.convertToCustomer}
          </SalesQueueButton>
        </div>
      </div>
    );
  }

  const customer = data.customers.find((item) => item.id === recordId);
  const customerOffer = customer?.offerId ? data.offers.find((item) => item.id === customer.offerId) : data.offers.find((item) => item.id === recordId && item.status === "Accepted");
  if (panel === "customer" && (customer || customerOffer)) {
    const account = customerOffer ? data.accounts.find((item) => item.id === customerOffer.accountId) : undefined;
    const contact = customerOffer ? data.contacts.find((item) => item.id === customerOffer.contactId) : undefined;
    const opportunityForCustomer = customerOffer ? data.opportunities.find((item) => item.id === customerOffer.opportunityId) : undefined;
    const displayCustomer = customer ?? (customerOffer ? customerFromOffer(customerOffer, account?.name, contact?.name, contact?.email) : null);
    if (!displayCustomer) return null;

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={displayCustomer.name} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(displayCustomer.summary, locale)}</p>
        <dl className="drawer-facts">
          <Fact label={copy.customer} value={displayCustomer.name} />
          <Fact label={copy.contact} value={displayCustomer.contactName} />
          <Fact label={copy.email} value={displayCustomer.email} />
          <Fact label={copy.source} value={displayCustomer.source} />
          <Fact label={copy.offer} value={customerOffer ? `${customerOffer.number} · ${customerOffer.title}` : "-"} />
          <Fact label={copy.onboarding} value={displayCustomer.onboardingStatus} />
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.handoffScope}</h3>
          <div className="ops-mini-list">
            <span>{customerOffer ? text(customerOffer.deliveryScope, locale) : text(displayCustomer.nextStep, locale)}</span>
            <span>{customerOffer ? text(customerOffer.paymentTerms, locale) : copy.directCustomerNoPrerequisite}</span>
            <span>{copy.operationsTakesOver}</span>
          </div>
        </section>
        <SalesQueueButton
          action="convert"
          className="drawer-primary"
          instruction={`Create or update an Operations onboarding project for Sales customer ${displayCustomer.name}. Prior campaign, pipeline, lead, or offer records are optional; preserve links when present.`}
          payload={{ customer: displayCustomer, offer: customerOffer, account, contact, opportunity: opportunityForCustomer }}
          recordId={displayCustomer.id}
          resource="onboarding_projects"
          title={`Create onboarding project: ${displayCustomer.name}`}
        >
          {copy.createOnboardingProject}
        </SalesQueueButton>
      </div>
    );
  }

  const account = data.accounts.find((item) => item.id === recordId);
  if (panel === "account" && account) {
    const owner = data.owners.find((item) => item.id === account.ownerId);
    const contacts = data.contacts.filter((item) => item.accountId === account.id);
    const opportunities = data.opportunities.filter((item) => item.accountId === account.id);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={account.name} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(account.summary, locale)}</p>
        <dl className="drawer-facts">
          <Fact label={copy.segment} value={account.segment} />
          <Fact label={copy.region} value={account.region} />
          <Fact label={copy.owner} value={owner?.name} />
          <Fact label={copy.health} value={account.health} />
          <Fact label={copy.value} value={formatMoney(account.annualValue)} />
          <Fact label={copy.renewal} value={account.renewalDate} />
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.nextStep}</h3>
          <div className="ops-mini-list"><span>{text(account.nextStep, locale)}</span></div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.relationships}</h3>
          <div className="ops-mini-list">
            {contacts.map((contact) => <span key={contact.id}>{contact.name} - {contact.relationship}</span>)}
            {opportunities.map((item) => <span key={item.id}>{item.name} - {item.stage} - {formatMoney(item.value)}</span>)}
          </div>
        </section>
        <SalesQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Synchronize won Sales lead ${account.name} with contacts, activity history, offer readiness, and customer handoff context.`}
          payload={{ account, contacts, opportunities }}
          recordId={account.id}
          resource="accounts"
          title={`Sync account: ${account.name}`}
        >
          {copy.askCtox}
        </SalesQueueButton>
        <SalesQueueButton
          action="convert"
          className="drawer-primary"
          instruction={`Create a Sales offer from lead ${account.name} when the current sales activity path has enough scope, contact, and commercial context. Prior campaign and pipeline records are optional.`}
          payload={{ account, contacts, opportunities }}
          recordId={`offer-from-${account.id}`}
          resource="offers"
          title={`Create offer from lead: ${account.name}`}
        >
          {copy.createOfferFromLead}
        </SalesQueueButton>
      </div>
    );
  }

  const contact = data.contacts.find((item) => item.id === recordId);
  if (panel === "contact" && contact) {
    const linkedAccount = data.accounts.find((item) => item.id === contact.accountId);
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={contact.name} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(contact.nextStep, locale)}</p>
        <dl className="drawer-facts">
          <Fact label={copy.account} value={linkedAccount?.name} />
          <Fact label={copy.role} value={contact.role} />
          <Fact label={copy.relationship} value={contact.relationship} />
          <Fact label={copy.email} value={contact.email} />
          <Fact label={copy.phone} value={contact.phone} />
          <Fact label={copy.lastTouch} value={contact.lastTouch} />
        </dl>
        <SalesQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Update contact intelligence for ${contact.name} and queue the next best sales action in CTOX.`}
          payload={{ contact, account: linkedAccount }}
          recordId={contact.id}
          resource="contacts"
          title={`Sync contact: ${contact.name}`}
        >
          {copy.askCtox}
        </SalesQueueButton>
      </div>
    );
  }

  const lead = data.leads.find((item) => item.id === recordId);
  if (panel === "lead" && lead) {
    const owner = data.owners.find((item) => item.id === lead.ownerId);
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={lead.company} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(lead.nextStep, locale)}</p>
        <dl className="drawer-facts">
          <Fact label={copy.contact} value={lead.contactName} />
          <Fact label={copy.role} value={lead.title} />
          <Fact label={copy.email} value={lead.email} />
          <Fact label={copy.source} value={lead.source} />
          <Fact label={copy.score} value={String(lead.score)} />
          <Fact label={copy.status} value={lead.status} />
          <Fact label={copy.owner} value={owner?.name} />
        </dl>
        <SalesQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Research, qualify, and route Sales lead ${lead.company} in CTOX.`}
          payload={{ lead }}
          recordId={lead.id}
          resource="leads"
          title={`Qualify lead: ${lead.company}`}
        >
          {copy.askCtox}
        </SalesQueueButton>
      </div>
    );
  }

  const task = data.tasks.find((item) => item.id === recordId);
  if (panel === "task" && task) {
    const owner = data.owners.find((item) => item.id === task.ownerId);
    const linked = describeLinkedRecord(data, task);
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={task.subject} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(task.nextStep, locale)}</p>
        <dl className="drawer-facts">
          <Fact label={copy.owner} value={owner?.name} />
          <Fact label={copy.due} value={task.due} />
          <Fact label={copy.priority} value={task.priority} />
          <Fact label={copy.status} value={task.status} />
          <Fact label={copy.linkedRecord} value={linked} />
        </dl>
        <SalesQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Update Sales task ${task.subject}, check linked CRM context, and queue the next action.`}
          payload={{ task, linkedRecord: linked }}
          recordId={task.id}
          resource="tasks"
          title={`Sync task: ${task.subject}`}
        >
          {copy.askCtox}
        </SalesQueueButton>
      </div>
    );
  }

  return null;
}

function PipelineView({ copy, data, locale, query, submoduleId }: SalesViewProps) {
  const totalValue = data.opportunities.reduce((sum, opportunity) => sum + opportunity.value, 0);
  const weighted = data.opportunities.reduce((sum, opportunity) => sum + (opportunity.value * opportunity.probability / 100), 0);
  const dueTasks = data.tasks.filter((task) => task.status !== "Done").sort((left, right) => left.due.localeCompare(right.due));

  return (
    <div className="ops-workspace sales-workspace sales-pipeline-workspace">
      <section className="ops-pane ops-project-tree" aria-label={copy.pipeline}>
        <PaneHead title={copy.pipeline} description={copy.pipelineDescription}>
          <a {...createContext(copy.newOpportunity, submoduleId, "opportunity")} href={salesPanelHref(query, submoduleId, "new", "opportunity", "left-bottom")}>+</a>
        </PaneHead>
        <div className="ops-signal-list">
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "open-pipeline", "right")} label={copy.openPipeline} value={formatMoney(totalValue)} />
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "weighted-forecast", "right")} label={copy.weightedForecast} value={formatMoney(Math.round(weighted))} />
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "next-close", "right")} label={copy.nextClose} value={nextCloseDate(data.opportunities)} />
        </div>
        <div className="ops-card-stack">
          {dueTasks.slice(0, 5).map((task) => (
            <a
              className={`ops-work-card priority-${task.priority.toLowerCase()}`}
              href={salesPanelHref(query, "tasks", "task", task.id, "right")}
              key={task.id}
              {...recordContext(task.subject, submoduleId, "task", task.id)}
            >
              <strong>{task.subject}</strong>
              <small>{task.due} - {ownerName(data, task.ownerId)}</small>
              <span>{text(task.nextStep, locale)}</span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-work-items" aria-label={copy.pipelineBoard}>
        <PaneHead title={copy.pipelineBoard} description={copy.pipelineBoardDescription} />
        <div className="os-lane-board sales-stage-board">
          {pipelineStages.map((stage) => {
            const stageItems = data.opportunities.filter((opportunity) => opportunity.stage === stage);
            return (
              <section key={stage} aria-label={stage}>
                <h2>{stage}</h2>
                <p>{formatMoney(stageItems.reduce((sum, opportunity) => sum + opportunity.value, 0))}</p>
                <div className="ops-card-stack">
                  {stageItems.map((opportunity) => {
                    const account = data.accounts.find((item) => item.id === opportunity.accountId);
                    return (
                      <a
                        className="ops-work-card"
                        href={salesPanelHref(query, submoduleId, "opportunity", opportunity.id, "right")}
                        key={opportunity.id}
                        {...recordContext(opportunity.name, submoduleId, "opportunity", opportunity.id)}
                      >
                        <strong>{opportunity.name}</strong>
                        <small>{account?.name} - {formatMoney(opportunity.value)} - {opportunity.probability}%</small>
                        <span>{text(opportunity.nextStep, locale)}</span>
                      </a>
                    );
                  })}
                </div>
              </section>
            );
          })}
        </div>
      </section>
    </div>
  );
}

function OffersView({ copy, data, locale, query, submoduleId }: SalesViewProps) {
  const selectedOfferId = query.selectedId ?? (query.panel === "offer" ? query.recordId : undefined);
  const selectedOffer = data.offers.find((offer) => offer.id === selectedOfferId) ?? data.offers[0];
  const openOffers = data.offers.filter((offer) => offer.status === "Draft" || offer.status === "Sent");
  const acceptedValue = data.offers.filter((offer) => offer.status === "Accepted").reduce((sum, offer) => sum + offer.grossAmount, 0);
  const expiring = data.offers.filter((offer) => offer.status === "Sent" && offer.validUntil <= "2026-05-15");
  const declined = data.offers.filter((offer) => offer.status === "Declined" || offer.status === "Expired");

  return (
    <div className="invoice-workspace invoice-editor-workspace sales-offer-editor-workspace">
      <aside className="ops-pane invoice-list-pane" aria-label={copy.offers}>
        <header className="invoice-list-head">
          <h2>{copy.offers}</h2>
          <a {...createContext(copy.newOffer, submoduleId, "offer")} href={salesPanelHref(query, submoduleId, "new", "offer", "left-bottom")}>+</a>
        </header>
        <div className="invoice-toolbar">
          <input aria-label={copy.searchOffers} className="invoice-search" placeholder={copy.searchOffers} type="search" />
        </div>
        <div className="invoice-filter-row">
          <a href={baseHref(query, submoduleId)}>{copy.all}</a>
          <a href={salesPanelHref(query, submoduleId, "sales-set", "offers-valid", "right")}>{copy.openOffers}</a>
          <a href={salesPanelHref(query, submoduleId, "sales-set", "offers-expiring", "right")}>{copy.expiringOffers}</a>
          <a href={salesPanelHref(query, submoduleId, "sales-set", "offers-accepted", "right")}>{copy.acceptedOffers}</a>
        </div>
        <a className="invoice-new-row" {...createContext(copy.newOffer, submoduleId, "offer")} href={salesPanelHref(query, submoduleId, "new", "offer", "left-bottom")}>+ {copy.newOffer}</a>
        <div className="invoice-compact-list">
          {data.offers.map((offer) => {
            const account = data.accounts.find((item) => item.id === offer.accountId);
            return (
              <a
                className={`invoice-compact-row ${selectedOffer?.id === offer.id ? "is-selected" : ""}`}
                href={salesSelectionHref(query, submoduleId, offer.id)}
                key={offer.id}
                {...recordContext(offer.title, submoduleId, "offer", offer.id)}
              >
                <span>
                  <strong>{account?.name ?? offer.accountId}</strong>
                  <small>{offer.number} - {offer.status} - {offer.title}</small>
                </span>
                <span>
                  <b>{formatMoney(offer.grossAmount, offer.currency, locale)}</b>
                  <small>{offer.status === "Sent" ? `${copy.validUntil}: ${offer.validUntil}` : offer.status}</small>
                </span>
              </a>
            );
          })}
        </div>
        <div className="invoice-list-metrics">
          <a className="invoice-list-metric" href={salesPanelHref(query, submoduleId, "sales-set", "offers-open", "right")}><span>{copy.openOffers}</span><strong>{String(openOffers.length)}</strong></a>
          <a className="invoice-list-metric" href={salesPanelHref(query, submoduleId, "sales-set", "offers-accepted", "right")}><span>{copy.acceptedOffers}</span><strong>{formatMoney(acceptedValue, "EUR", locale)}</strong></a>
          <a className="invoice-list-metric" href={salesPanelHref(query, submoduleId, "sales-set", "offers-expiring", "right")}><span>{copy.expiringOffers}</span><strong>{String(expiring.length)}</strong></a>
        </div>
      </aside>

      {selectedOffer ? (
        selectedOffer.status === "Draft" ? (
          <OfferEditor copy={copy} data={data} locale={locale} offer={selectedOffer} query={query} submoduleId={submoduleId} />
        ) : (
          <OfferPreviewPane copy={copy} data={data} locale={locale} offer={selectedOffer} query={query} submoduleId={submoduleId} declinedCount={declined.length} />
        )
      ) : null}
    </div>
  );
}

function OfferEditor({ copy, data, locale, offer, query, submoduleId }: {
  copy: Copy;
  data: SalesBundle;
  locale: SupportedLocale;
  offer: SalesOffer;
  query: QueryState;
  submoduleId: string;
}) {
  const account = data.accounts.find((item) => item.id === offer.accountId);
  const contact = data.contacts.find((item) => item.id === offer.contactId);
  const customerOptions: InvoiceCustomerOption[] = data.accounts.map((item) => {
    const accountContact = data.contacts.find((candidate) => candidate.accountId === item.id);
    return {
      addressExtra: accountContact?.name ?? "",
      city: item.region,
      country: item.region === "DACH" ? "Deutschland" : item.region,
      customerNumber: item.id === account?.id ? offer.accountId : "",
      id: item.id,
      name: item.name,
      postalCode: "",
      street: ""
    };
  });
  const lineDrafts: InvoiceLineDraft[] = offer.lineItems.map((line, index) => ({
    currency: offer.currency,
    description: line.description,
    id: `${offer.id}-${index}`,
    quantity: line.quantity,
    taxRate: line.taxRate,
    title: line.description,
    unit: offerUnitLabel(line.unit, copy),
    unitPrice: line.unitPrice
  }));

  return (
    <section className="ops-pane invoice-editor-pane offer-editor-pane" aria-label={copy.offer}>
      <div className="invoice-editor-topbar">
        <div>
          <h2>{offer.title} {copy.edit}</h2>
          <p>{offer.number} - {account?.name} - {offer.status}</p>
        </div>
        <div className="invoice-mode-toggle" aria-label={copy.status}>
          <span>{copy.gross}</span>
          <strong>{copy.net}</strong>
        </div>
      </div>
      <div className="invoice-editor-scroll">
        <InvoiceCustomerEditor
          copy={copy}
          customers={customerOptions}
          initialCustomerId={account?.id ?? customerOptions[0]?.id ?? "manual"}
          invoice={{
            dueDate: offer.validUntil,
            issueDate: offer.issuedAt,
            number: offer.number,
            serviceDate: offer.issuedAt
          }}
        />

        <section className="invoice-editor-card invoice-text-card">
          <OfferField label={copy.documentTitle} value={offer.title} />
          <LexicalRichTextEditor
            initialText={text(offer.introText, locale)}
            label={copy.introText}
            locale={locale}
            namespace={`offer-${offer.id}-intro`}
            placeholder={copy.offerIntroTextPlaceholder}
            templateHref={salesPanelHref(query, submoduleId, "text-template", "introText", "right")}
          />
        </section>

        <InvoiceLinesEditor copy={copy} initialLines={lineDrafts} locale={locale} />

        <section className="invoice-editor-card invoice-text-card">
          <LexicalRichTextEditor
            initialText={text(offer.paymentTerms, locale)}
            label={copy.paymentTerms}
            locale={locale}
            namespace={`offer-${offer.id}-payment-terms`}
            placeholder={copy.paymentConditionPlaceholder}
            templateHref={salesPanelHref(query, submoduleId, "text-template", "paymentTerms", "right")}
          />
          <LexicalRichTextEditor
            initialText={text(offer.closingText, locale)}
            label={copy.closingText}
            locale={locale}
            namespace={`offer-${offer.id}-closing`}
            placeholder={copy.closingNotePlaceholder}
            templateHref={salesPanelHref(query, submoduleId, "text-template", "closingText", "right")}
          />
        </section>
      </div>
      <div className="invoice-editor-footer">
        <a href={salesPanelHref(query, submoduleId, "offer", offer.id, "right")}>{copy.moreDetails}</a>
        <SalesQueueButton
          action="sync"
          instruction={`Save and review Sales offer draft ${offer.number}. Keep customer, text blocks, line items, tax, validity, and customer handoff readiness aligned.`}
          payload={{ offer, account, contact }}
          recordId={offer.id}
          resource="offers"
          title={`Save offer draft: ${offer.number}`}
        >
          {copy.saveDraft}
        </SalesQueueButton>
        <SalesQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Send Sales offer ${offer.number} after owner review. Preserve document texts, line items, tax, validity, and reply/customer handoff context.`}
          payload={{ offer, account, contact }}
          recordId={offer.id}
          resource="offers"
          title={`Send offer: ${offer.number}`}
        >
          {copy.sendOffer}
        </SalesQueueButton>
      </div>
    </section>
  );
}

function OfferPreviewPane({ copy, data, locale, offer, query, submoduleId, declinedCount }: {
  copy: Copy;
  data: SalesBundle;
  locale: SupportedLocale;
  offer: SalesOffer;
  query: QueryState;
  submoduleId: string;
  declinedCount: number;
}) {
  const account = data.accounts.find((item) => item.id === offer.accountId);
  const contact = data.contacts.find((item) => item.id === offer.contactId);

  return (
    <section className="ops-pane invoice-editor-pane invoice-preview-editor-pane offer-preview-editor-pane" aria-label={copy.offerPreview}>
      <div className="invoice-editor-topbar">
        <div>
          <h2>{copy.offer} {offer.number}</h2>
          <p>{account?.name} - {offer.status} - {copy.validUntil}: {offer.validUntil}</p>
        </div>
        <div className="invoice-mode-toggle" aria-label={copy.status}>
          <strong>{copy.preview}</strong>
        </div>
      </div>
      <div className="invoice-preview-scroll">
        <div className="offer-document-preview" {...recordContext(offer.title, submoduleId, "offer", offer.id)}>
          <header>
            <span>{copy.offer}</span>
            <strong>{offer.number}</strong>
          </header>
          <address>
            {account?.name}<br />
            {contact?.name}<br />
            {contact?.email}
          </address>
          <h3>{offer.title}</h3>
          <p>{text(offer.introText, locale)}</p>
          <div className="offer-preview-lines">
            {offer.lineItems.map((line) => (
              <span key={line.description}>
                <b>{line.description}</b>
                <small>{line.quantity} {line.unit} x {formatMoney(line.unitPrice, offer.currency, locale)} · {line.taxRate}%</small>
                <em>{formatMoney(line.quantity * line.unitPrice, offer.currency, locale)}</em>
              </span>
            ))}
          </div>
          <p>{text(offer.paymentTerms, locale)}</p>
          <p>{text(offer.closingText, locale)}</p>
          <footer>
            <span>{copy.gross}</span>
            <strong>{formatMoney(offer.grossAmount, offer.currency, locale)}</strong>
          </footer>
        </div>
        <div className="invoice-detail-facts">
          <div><dt>{copy.status}</dt><dd>{offer.status}</dd></div>
          <div><dt>{copy.net}</dt><dd>{formatMoney(offer.netAmount, offer.currency, locale)}</dd></div>
          <div><dt>{copy.tax}</dt><dd>{formatMoney(offer.taxAmount, offer.currency, locale)}</dd></div>
          <div><dt>{copy.declinedOffers}</dt><dd>{declinedCount}</dd></div>
        </div>
      </div>
      <div className="invoice-editor-footer">
        <a href={salesPanelHref(query, submoduleId, "offer", offer.id, "right")}>{copy.moreDetails}</a>
        <SalesQueueButton
          action="sync"
          instruction={`Review Sales offer ${offer.number}, check Lexoffice-style offer fields, and prepare customer plus Operations onboarding handoff if accepted.`}
          payload={{ offer, account, contact }}
          recordId={offer.id}
          resource="offers"
          title={`Review offer: ${offer.number}`}
        >
          {copy.askCtox}
        </SalesQueueButton>
        <SalesQueueButton
          action="convert"
          className="drawer-primary"
          instruction={`Convert accepted Sales offer ${offer.number} into a customer record and queue Operations onboarding. Sales offers are optional prerequisites; preserve offer, customer, scope, contact, and payment terms.`}
          payload={{ offer, account, contact }}
          recordId={offer.id}
          resource="offers"
          title={`Convert offer to customer: ${offer.number}`}
        >
          {copy.convertToCustomer}
        </SalesQueueButton>
      </div>
    </section>
  );
}

function OfferField({ label, value }: { label: string; value?: string }) {
  return (
    <div className="invoice-field">
      <span>{label}</span>
      <strong>{value ?? "-"}</strong>
    </div>
  );
}

function LeadAccountsView({ copy, data, locale, query, submoduleId }: SalesViewProps) {
  const activeLeadAccounts = data.accounts.filter((account) => {
    const acceptedOffer = data.offers.some((offer) => offer.accountId === account.id && offer.status === "Accepted");
    return !acceptedOffer;
  });
  const normalizedSearch = query.search?.trim().toLowerCase() ?? "";
  const stageFilter = query.stage ?? "all";
  const visibleLeadAccounts = activeLeadAccounts.filter((account) => {
    const opportunities = data.opportunities.filter((opportunity) => opportunity.accountId === account.id);
    const contact = data.contacts.find((item) => item.accountId === account.id);
    const offer = data.offers.find((item) => item.accountId === account.id);
    const stageLabel = offer?.status ?? opportunities[0]?.stage ?? "";
    const searchHaystack = [
      account.name,
      account.segment,
      account.region,
      contact?.name,
      contact?.role,
      text(account.nextStep, locale),
      stageLabel
    ].join(" ").toLowerCase();
    const matchesSearch = !normalizedSearch || searchHaystack.includes(normalizedSearch);
    const matchesStage = stageFilter === "all"
      || (stageFilter === "ready" && (offer?.status === "Draft" || offer?.status === "Sent" || opportunities.some((opportunity) => opportunity.stage === "Proposal" || opportunity.stage === "Negotiation")))
      || (stageFilter === "active" && !offer)
      || stageLabel.toLowerCase() === stageFilter.toLowerCase();
    return matchesSearch && matchesStage;
  });
  const selectedLeadId = query.selectedId ?? (query.panel === "account" ? query.recordId : undefined);
  const selectedLead = activeLeadAccounts.find((account) => account.id === selectedLeadId) ?? visibleLeadAccounts[0] ?? activeLeadAccounts[0];
  const selectedContacts = selectedLead ? data.contacts.filter((contact) => contact.accountId === selectedLead.id) : [];
  const selectedContact = selectedContacts[0];
  const selectedOpportunities = selectedLead ? data.opportunities.filter((opportunity) => opportunity.accountId === selectedLead.id) : [];
  const selectedOpportunity = selectedOpportunities.find((opportunity) => opportunity.stage !== "Won") ?? selectedOpportunities[0];
  const selectedOffer = selectedLead ? data.offers.find((offer) => offer.accountId === selectedLead.id) : undefined;
  const selectedTasks = selectedLead ? tasksForLeadAccount(data, selectedLead) : [];
  const flow = selectedLead ? buildLeadAccessFlow({
    account: selectedLead,
    contact: selectedContact,
    contacts: selectedContacts,
    locale,
    offer: selectedOffer,
    opportunity: selectedOpportunity,
    tasks: selectedTasks
  }) : { links: [], nodes: [], patterns: [] };
  const flowDone = flow.nodes.filter((node) => node.state === "done").length;
  const flowRunning = flow.nodes.filter((node) => node.state === "running").length;
  const flowPlanned = flow.nodes.filter((node) => node.state === "planned").length;
  const flowBlocked = flow.nodes.filter((node) => node.state === "blocked").length;

  if (!selectedLead) {
    return (
      <div className="ops-workspace sales-workspace">
        <section className="ops-pane ops-work-items" aria-label={copy.leads}>
          <PaneHead title={copy.leads} description={copy.leadsDescription}>
            <a {...createContext(copy.newLead, submoduleId, "account")} href={salesPanelHref(query, submoduleId, "new", "account", "left-bottom")}>+</a>
          </PaneHead>
        </section>
      </div>
    );
  }

  return (
    <div className="lead-flow-workspace">
      <section className="ops-pane lead-selector-pane" aria-label={copy.leads}>
        <header className="lead-selector-head">
          <div>
            <h2>{copy.leads}</h2>
            <p>{locale === "de" ? "Lead-Accounts vor dem Angebot. Jeder Lead kann direkt hier starten." : "Lead accounts before offer creation. Each lead can start here directly."}</p>
          </div>
          <a {...createContext(copy.newLead, submoduleId, "account")} href={salesPanelHref(query, submoduleId, "new", "account", "left-bottom")}>+</a>
        </header>
        <form action={`/app/sales/${submoduleId}`} className="lead-selector-filter" method="get">
          {query.locale ? <input name="locale" type="hidden" value={query.locale} /> : null}
          {query.theme ? <input name="theme" type="hidden" value={query.theme} /> : null}
          {selectedLead?.id ? <input name="selectedId" type="hidden" value={selectedLead.id} /> : null}
          <label>
            <span>{locale === "de" ? "Suche" : "Search"}</span>
            <input defaultValue={query.search ?? ""} name="search" placeholder={locale === "de" ? "Firma, Kontakt, Schritt ..." : "Company, contact, step ..."} />
          </label>
          <label>
            <span>{locale === "de" ? "Filter" : "Filter"}</span>
            <select defaultValue={stageFilter} name="stage">
              <option value="all">{locale === "de" ? "Alle Lead-Staende" : "All lead states"}</option>
              <option value="active">{locale === "de" ? "ohne Angebot" : "without offer"}</option>
              <option value="ready">{locale === "de" ? "angebotesreif" : "offer ready"}</option>
              <option value="Draft">Draft</option>
              <option value="Sent">Sent</option>
              <option value="Expired">Expired</option>
              <option value="Discover">Discovery</option>
              <option value="Proposal">Proposal</option>
            </select>
          </label>
          <button type="submit">{locale === "de" ? "Anwenden" : "Apply"}</button>
        </form>
        <div className="lead-selector-list">
          {visibleLeadAccounts.map((account) => {
            const opportunities = data.opportunities.filter((opportunity) => opportunity.accountId === account.id);
            const contact = data.contacts.find((item) => item.accountId === account.id);
            const offer = data.offers.find((item) => item.accountId === account.id);
            return (
              <a
                className={`lead-selector-card ${account.id === selectedLead.id ? "is-selected" : ""}`}
                href={salesSelectionHref(query, submoduleId, account.id)}
                key={account.id}
                {...recordContext(account.name, submoduleId, "account", account.id)}
              >
                <span>
                  <strong>{account.name}</strong>
                  <small>{contact?.name ?? copy.contact} - {account.segment}</small>
                </span>
                <em>{offer?.status ?? opportunityStageLabel(opportunities[0]?.stage, locale)}</em>
                <small>{text(account.nextStep, locale)}</small>
                <b>{formatMoney(account.annualValue, "EUR", locale)}</b>
              </a>
            );
          })}
        </div>
      </section>

      <section className="ops-pane lead-flow-pane" aria-label={locale === "de" ? "Lead Flow" : "Lead flow"}>
        <header className="lead-flow-head">
          <div>
            <span>{locale === "de" ? "Zugangs-Map" : "Access map"}</span>
            <h2>{selectedLead.name}</h2>
            <p>{selectedLead.summary ? text(selectedLead.summary, locale) : copy.leadsDescription}</p>
          </div>
          <div className="lead-flow-status">
            <Signal label={locale === "de" ? "erfolgreich" : "done"} value={String(flowDone)} />
            <Signal label={locale === "de" ? "laufend" : "running"} value={String(flowRunning)} />
            <Signal label={locale === "de" ? "geplant" : "planned"} value={String(flowPlanned)} />
            <Signal label={locale === "de" ? "blockiert" : "blocked"} value={String(flowBlocked)} />
          </div>
        </header>
        <div className="lead-flow-canvas-wrap">
          <LeadFlowCanvas
            createLeadHref={salesPanelHref(query, submoduleId, "new", "account", "left-bottom")}
            flow={flow}
            locale={locale}
            storageKey={selectedLead.id}
          />
        </div>
        <footer className="lead-flow-command-bar">
          <SalesQueueButton
            action="create"
            className="drawer-primary"
            instruction={`Attach a user-planned next action to the lead access map for ${selectedLead.name}. Keep the action as a node with owner, trigger, due date, expected result, and relation to the current flow.`}
            payload={{ account: selectedLead, contact: selectedContact, flow }}
            recordId={`lead-step-${selectedLead.id}`}
            resource="sales_activity"
            title={`Attach user step to lead flow: ${selectedLead.name}`}
          >
            {locale === "de" ? "Planschritt anhaengen" : "Attach plan step"}
          </SalesQueueButton>
          <SalesQueueButton
            action="sync"
            className="campaign-secondary"
            instruction={`Research and plan the next two best access steps for ${selectedLead.name}. Use web research, CRM context, previous touchpoints, stakeholder gaps, and current opportunity risk. Return explicit nodes and links for the flow map.`}
            payload={{ account: selectedLead, contacts: selectedContacts, opportunity: selectedOpportunity, tasks: selectedTasks, flow }}
            recordId={`lead-agent-plan-${selectedLead.id}`}
            resource="sales_activity"
            title={`Agent plan for lead flow: ${selectedLead.name}`}
          >
            {locale === "de" ? "Agent plant naechste Schritte" : "Agent plans next steps"}
          </SalesQueueButton>
          <SalesQueueButton
            action="delete"
            className="campaign-secondary"
            instruction={`Remove or archive obsolete planned steps from the lead access map for ${selectedLead.name}. Keep completed evidence, but mark superseded plans as removed with reason.`}
            payload={{ account: selectedLead, flow }}
            recordId={`lead-stale-plan-${selectedLead.id}`}
            resource="sales_activity"
            title={`Remove stale lead flow plan: ${selectedLead.name}`}
          >
            {locale === "de" ? "Ueberholten Plan entfernen" : "Remove stale plan"}
          </SalesQueueButton>
        </footer>
      </section>

    </div>
  );
}

function ContactsView({ copy, data, locale, query, submoduleId }: SalesViewProps) {
  return (
    <div className="ops-workspace sales-workspace sales-contacts-workspace">
      <section className="ops-pane ops-work-items" aria-label={copy.contacts}>
        <PaneHead title={copy.contacts} description={copy.contactsDescription}>
          <a {...createContext(copy.newContact, submoduleId, "contact")} href={salesPanelHref(query, submoduleId, "new", "contact", "left-bottom")}>+</a>
        </PaneHead>
        <div className="ops-table ops-work-table">
          <div className="ops-table-head">
            <span>{copy.contact}</span>
            <span>{copy.account}</span>
            <span>{copy.relationship}</span>
            <span>{copy.nextStep}</span>
          </div>
          {data.contacts.map((contact) => {
            const account = data.accounts.find((item) => item.id === contact.accountId);
            return (
              <a
                className="ops-table-row"
                href={salesPanelHref(query, submoduleId, "contact", contact.id, "right")}
                key={contact.id}
                {...recordContext(contact.name, submoduleId, "contact", contact.id)}
              >
                <span><strong>{contact.name}</strong><small>{contact.role} - {contact.email}</small></span>
                <span><strong>{account?.name}</strong><small>{account?.segment}</small></span>
                <span><strong>{contact.relationship}</strong><small>{copy.lastTouch}: {contact.lastTouch}</small></span>
                <span><strong>{text(contact.nextStep, locale)}</strong><small>{contact.phone}</small></span>
              </a>
            );
          })}
        </div>
      </section>
      <section className="ops-pane ops-sync-rail" aria-label={copy.relationships}>
        <PaneHead title={copy.relationships} description={copy.relationshipsDescription} />
        <div className="ops-signal-list">
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "champions", "right")} label={copy.champions} value={String(data.contacts.filter((contact) => contact.relationship === "Champion").length)} />
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "decision-makers", "right")} label={copy.decisionMakers} value={String(data.contacts.filter((contact) => contact.relationship === "Decision maker").length)} />
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "accounts", "right")} label={copy.accounts} value={String(data.accounts.length)} />
        </div>
      </section>
    </div>
  );
}

function LeadsView({ copy, data, locale, query, submoduleId }: SalesViewProps) {
  const statuses: SalesLead["status"][] = ["New", "Research", "Qualified", "Nurture"];
  return (
    <div className="ops-workspace sales-workspace sales-leads-workspace">
      <section className="ops-pane ops-work-items" aria-label={copy.leads}>
        <PaneHead title={copy.leads} description={copy.leadsDescription}>
          <a {...createContext(copy.newLead, submoduleId, "lead")} href={salesPanelHref(query, submoduleId, "new", "lead", "left-bottom")}>+</a>
        </PaneHead>
        <div className="os-lane-board sales-stage-board">
          {statuses.map((status) => (
            <section key={status} aria-label={status}>
              <h2>{status}</h2>
              <p>{data.leads.filter((lead) => lead.status === status).length} {copy.items}</p>
              <div className="ops-card-stack">
                {data.leads.filter((lead) => lead.status === status).map((lead) => (
                  <a
                    className="ops-work-card"
                    href={salesPanelHref(query, submoduleId, "lead", lead.id, "right")}
                    key={lead.id}
                    {...recordContext(lead.company, submoduleId, "lead", lead.id)}
                  >
                    <strong>{lead.company}</strong>
                    <small>{lead.contactName} - {lead.source} - {copy.score} {lead.score}</small>
                    <span>{text(lead.nextStep, locale)}</span>
                  </a>
                ))}
              </div>
            </section>
          ))}
        </div>
      </section>
      <section className="ops-pane ops-sync-rail" aria-label={copy.leadSignals}>
        <PaneHead title={copy.leadSignals} description={copy.leadSignalsDescription} />
        <div className="ops-card-stack">
          {data.leads.sort((left, right) => right.score - left.score).slice(0, 4).map((lead) => (
            <a
              className="ops-work-card priority-high"
              href={salesPanelHref(query, submoduleId, "lead", lead.id, "right")}
              key={lead.id}
              {...recordContext(lead.company, submoduleId, "lead", lead.id)}
            >
              <strong>{lead.company}</strong>
              <small>{copy.score} {lead.score} - {lead.source}</small>
              <span>{text(lead.nextStep, locale)}</span>
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

function CustomersView({ copy, data, locale, query, submoduleId }: SalesViewProps) {
  const customerRows = data.customers.map((customer) => {
    const offer = customer.offerId ? data.offers.find((item) => item.id === customer.offerId) : undefined;
    const opportunity = offer ? data.opportunities.find((item) => item.id === offer.opportunityId) : undefined;
    const account = offer ? data.accounts.find((item) => item.id === offer.accountId) : undefined;
    const contact = offer ? data.contacts.find((item) => item.id === offer.contactId) : undefined;
    return { account, contact, customer, offer, opportunity };
  });
  const selectedCustomerId = query.selectedId ?? (query.panel === "customer" ? query.recordId : undefined);
  const selectedRow = customerRows.find((row) => row.customer.id === selectedCustomerId) ?? customerRows[0];
  const directCustomers = customerRows.filter((row) => !row.offer);
  const selectedBuyingCenter = selectedRow ? buyingCenterForCustomer(data, selectedRow) : [];
  const selectedDossierGaps = selectedRow ? customerDossierGaps(selectedRow, selectedBuyingCenter, locale) : [];

  return (
    <div className="sales-customers-command">
      <section className="ops-pane sales-customer-list-pane" aria-label={copy.customers}>
        <header className="invoice-list-head">
          <div>
            <h2>{copy.customers}</h2>
            <p>{copy.customersDescription}</p>
          </div>
          <a {...createContext(copy.newCustomer, submoduleId, "customer")} href={salesPanelHref(query, submoduleId, "new", "customer", "left-bottom")}>+</a>
        </header>
        <div className="sales-customer-metrics">
          <Signal label={copy.customers} value={String(customerRows.length)} />
          <Signal label={locale === "de" ? "Buying Center" : "Buying center"} value={String(data.contacts.length)} />
          <Signal label={locale === "de" ? "Direkt angelegt" : "Direct"} value={String(directCustomers.length)} />
          <Signal label={locale === "de" ? "Dossiers" : "Dossiers"} value={String(customerRows.length)} />
        </div>
        <div className="sales-customer-table">
          <div className="sales-customer-row sales-customer-head">
            <span>{copy.customer}</span>
            <span>{copy.source}</span>
            <span>{copy.offer}</span>
            <span>{locale === "de" ? "Intelligence" : "Intelligence"}</span>
          </div>
          {customerRows.map(({ customer, offer, opportunity }) => (
            <a
              className={`sales-customer-row ${selectedRow?.customer.id === customer.id ? "is-selected" : ""}`}
              href={salesSelectionHref(query, submoduleId, customer.id)}
              key={customer.id}
              {...recordContext(customer.name, submoduleId, "customer", customer.id)}
            >
              <span><strong>{customer.name}</strong><small>{customer.contactName} - {customer.segment}</small></span>
              <span><strong>{customer.source}</strong><small>{customer.email}</small></span>
              <span><strong>{offer?.number ?? copy.noPrerequisite}</strong><small>{offer ? `${offer.title} - ${formatMoney(offer.grossAmount, offer.currency, locale)}` : customer.source}</small></span>
              <span><strong>{opportunity?.name ?? (locale === "de" ? "Direktkunde" : "Direct customer")}</strong><small>{text(customer.nextStep, locale)}</small></span>
            </a>
          ))}
        </div>
      </section>

      {selectedRow ? (
        <section className="ops-pane sales-customer-detail-pane" aria-label={copy.customerHandoff}>
          <div className="invoice-editor-topbar">
            <div>
              <h2>{selectedRow.customer.name}</h2>
              <p>{locale === "de" ? "Stammdaten und Customer-Intelligence-Dossier" : "Master data and customer intelligence dossier"}</p>
            </div>
            <div className="invoice-mode-toggle" aria-label={copy.status}>
              <strong>{selectedRow.offer ? copy.acceptedOffers : (locale === "de" ? "Direktkunde" : "Direct customer")}</strong>
            </div>
          </div>

          <div className="sales-customer-detail-scroll">
            <section className="sales-customer-profile">
              <h3>{locale === "de" ? "Stammdaten" : "Master data"}</h3>
              <p>{text(selectedRow.customer.summary, locale)}</p>
              <dl>
                <div><dt>{copy.contact}</dt><dd>{selectedRow.customer.contactName}</dd></div>
                <div><dt>{copy.email}</dt><dd>{selectedRow.customer.email}</dd></div>
                <div><dt>{copy.owner}</dt><dd>{ownerName(data, selectedRow.customer.ownerId)}</dd></div>
                <div><dt>{copy.segment}</dt><dd>{selectedRow.customer.segment}</dd></div>
                <div><dt>{copy.source}</dt><dd>{selectedRow.customer.source}</dd></div>
                <div><dt>{locale === "de" ? "Dossier-Status" : "Dossier status"}</dt><dd>{selectedDossierGaps.length === 0 ? (locale === "de" ? "belastbar" : "solid") : (locale === "de" ? "zu vervollstaendigen" : "needs enrichment")}</dd></div>
              </dl>
            </section>

            <section className="sales-customer-profile">
              <h3>{locale === "de" ? "Kundenkontext" : "Customer context"}</h3>
              <div className="sales-customer-origin">
                <strong>{selectedRow.offer ? `${selectedRow.offer.number} - ${selectedRow.offer.title}` : copy.directCustomerNoPrerequisite}</strong>
                <span>{selectedRow.offer ? formatMoney(selectedRow.offer.grossAmount, selectedRow.offer.currency, locale) : selectedRow.customer.source}</span>
                <p>{selectedRow.offer ? text(selectedRow.offer.deliveryScope, locale) : text(selectedRow.customer.nextStep, locale)}</p>
                <small>{selectedRow.offer ? text(selectedRow.offer.paymentTerms, locale) : copy.noPrerequisite}</small>
              </div>
            </section>

            <section className="sales-customer-profile">
              <h3>{locale === "de" ? "Buying-Center-Mindmap" : "Buying center mind map"}</h3>
              <div className="customer-intelligence-map">
                <div className="customer-map-core">
                  <strong>{selectedRow.customer.name}</strong>
                  <span>{locale === "de" ? "Kundenakte" : "Customer dossier"}</span>
                </div>
                <div className="customer-map-ring">
                  {selectedBuyingCenter.map((contact) => (
                    <div className={`customer-map-node is-${contact.relationship.toLowerCase().replaceAll(" ", "-")}`} key={contact.id}>
                      <strong>{contact.name}</strong>
                      <span>{contact.role}</span>
                      <small>{contact.relationship} · {copy.lastTouch}: {contact.lastTouch}</small>
                    </div>
                  ))}
                  {selectedBuyingCenter.length === 0 ? (
                    <div className="customer-map-node is-open">
                      <strong>{locale === "de" ? "Buying Center offen" : "Buying center open"}</strong>
                      <span>{locale === "de" ? "Ansprechpartner und Entscheider muessen recherchiert werden." : "Stakeholders and decision makers need research."}</span>
                    </div>
                  ) : null}
                  <div className="customer-map-node is-operations">
                    <strong>{locale === "de" ? "Operations" : "Operations"}</strong>
                    <span>{selectedRow.customer.onboardingStatus}</span>
                    <small>{copy.operationsTakesOver}</small>
                  </div>
                </div>
              </div>
            </section>

            <section className="sales-customer-profile">
              <h3>{locale === "de" ? "Intelligence-Akte" : "Intelligence file"}</h3>
              <div className="customer-dossier-grid">
                <section>
                  <h4>{locale === "de" ? "Bekannt" : "Known"}</h4>
                  <span>{text(selectedRow.customer.nextStep, locale)}</span>
                  <span>{selectedRow.offer ? text(selectedRow.offer.deliveryScope, locale) : text(selectedRow.customer.summary, locale)}</span>
                  <span>{selectedRow.contact ? `${selectedRow.contact.name}: ${selectedRow.contact.relationship}` : selectedRow.customer.contactName}</span>
                </section>
                <section>
                  <h4>{locale === "de" ? "Zu klaeren" : "To enrich"}</h4>
                  {selectedDossierGaps.map((gap) => <span key={gap}>{gap}</span>)}
                </section>
                <section>
                  <h4>{locale === "de" ? "Operations-Relevanz" : "Operations relevance"}</h4>
                  <span>{selectedRow.offer ? text(selectedRow.offer.deliveryScope, locale) : copy.directCustomerNoPrerequisite}</span>
                  <span>{locale === "de" ? "Kaufentscheidung, Scope und Ansprechpartner bleiben im Dossier nachvollziehbar." : "Buying decision, scope, and stakeholders remain traceable in the dossier."}</span>
                </section>
              </div>
            </section>
          </div>

          <div className="invoice-editor-footer">
            <a href={salesPanelHref(query, submoduleId, "customer", selectedRow.customer.id, "right")}>{copy.moreDetails}</a>
            <SalesQueueButton
              action="sync"
              instruction={`Enrich the customer dossier for ${selectedRow.customer.name}. Research and structure master data, buying center, relevant stakeholders for purchase decisions, current scope, open risks, and Operations handoff context. Return dossier sections, relationship map updates, and missing information prompts.`}
              payload={{ customer: selectedRow.customer, offer: selectedRow.offer, account: selectedRow.account, contact: selectedRow.contact, buyingCenter: selectedBuyingCenter, gaps: selectedDossierGaps }}
              recordId={selectedRow.customer.id}
              resource="customers"
              title={`Enrich customer dossier: ${selectedRow.customer.name}`}
            >
              {locale === "de" ? "Dossier vervollstaendigen" : "Enrich dossier"}
            </SalesQueueButton>
          </div>
        </section>
      ) : null}
    </div>
  );
}

function buyingCenterForCustomer(
  data: SalesBundle,
  row: {
    account?: SalesAccount;
    contact?: SalesContact;
    customer: SalesCustomer;
  }
) {
  const contacts = row.account
    ? data.contacts.filter((contact) => contact.accountId === row.account?.id)
    : data.contacts.filter((contact) => contact.email === row.customer.email || contact.name === row.customer.contactName);
  const combined = [...contacts, row.contact].filter(Boolean) as SalesContact[];
  return [...new Map(combined.map((contact) => [contact.id, contact])).values()];
}

function customerDossierGaps(
  row: {
    account?: SalesAccount;
    contact?: SalesContact;
    customer: SalesCustomer;
    offer?: SalesOffer;
  },
  buyingCenter: SalesContact[],
  locale: SupportedLocale
) {
  const gaps: string[] = [];
  const hasDecisionMaker = buyingCenter.some((contact) => contact.relationship === "Decision maker");
  const hasChampion = buyingCenter.some((contact) => contact.relationship === "Champion");
  if (!hasDecisionMaker) gaps.push(locale === "de" ? "Entscheider im Buying Center bestaetigen." : "Confirm decision maker in buying center.");
  if (!hasChampion) gaps.push(locale === "de" ? "Champion oder internen Treiber identifizieren." : "Identify champion or internal driver.");
  if (!row.offer) gaps.push(locale === "de" ? "Kommerziellen Ursprung und vereinbarten Scope dokumentieren." : "Document commercial origin and agreed scope.");
  if (!row.account) gaps.push(locale === "de" ? "Account-Stammdaten und Organisationskontext ergaenzen." : "Enrich account master data and organization context.");
  if (row.customer.onboardingStatus === "Not started") gaps.push(locale === "de" ? "Operations-relevante Ansprechpartner fuer die Folgephase festhalten." : "Capture Operations-relevant stakeholders for the next phase.");
  return gaps.length > 0 ? gaps : [locale === "de" ? "Keine kritischen Dossier-Luecken sichtbar." : "No critical dossier gaps visible."];
}

function TasksView({ copy, data, locale, query, submoduleId }: SalesViewProps) {
  const tasks = [...data.tasks].sort((left, right) => left.due.localeCompare(right.due));
  return (
    <div className="ops-workspace sales-workspace sales-tasks-workspace">
      <section className="ops-pane ops-work-items" aria-label={copy.tasks}>
        <PaneHead title={copy.tasks} description={copy.tasksDescription}>
          <a {...createContext(copy.newTask, submoduleId, "task")} href={salesPanelHref(query, submoduleId, "new", "task", "left-bottom")}>+</a>
        </PaneHead>
        <div className="ops-table ops-work-table">
          <div className="ops-table-head">
            <span>{copy.task}</span>
            <span>{copy.owner}</span>
            <span>{copy.due}</span>
            <span>{copy.linkedRecord}</span>
          </div>
          {tasks.map((task) => (
            <a
              className="ops-table-row"
              href={salesPanelHref(query, submoduleId, "task", task.id, "right")}
              key={task.id}
              {...recordContext(task.subject, submoduleId, "task", task.id)}
            >
              <span><strong>{task.subject}</strong><small>{task.priority} - {task.status}</small></span>
              <span><strong>{ownerName(data, task.ownerId)}</strong><small>{text(task.nextStep, locale)}</small></span>
              <span><strong>{task.due}</strong><small>{task.status}</small></span>
              <span><strong>{describeLinkedRecord(data, task)}</strong><small>{task.linkedResource}</small></span>
            </a>
          ))}
        </div>
      </section>
      <section className="ops-pane ops-sync-rail" aria-label={copy.taskPressure}>
        <PaneHead title={copy.taskPressure} description={copy.taskPressureDescription} />
        <div className="ops-signal-list">
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "urgent-tasks", "right")} label={copy.urgent} value={String(tasks.filter((task) => task.priority === "Urgent").length)} />
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "open-tasks", "right")} label={copy.open} value={String(tasks.filter((task) => task.status !== "Done").length)} />
          <Signal href={salesPanelHref(query, submoduleId, "sales-set", "waiting-tasks", "right")} label={copy.waiting} value={String(tasks.filter((task) => task.status === "Waiting").length)} />
        </div>
      </section>
    </div>
  );
}

function AccountRail({ copy, data, query, submoduleId }: {
  copy: Copy;
  data: SalesBundle;
  query: QueryState;
  submoduleId: string;
}) {
  const riskyAccounts = data.accounts.filter((account) => account.health !== "Green");
  return (
    <section className="ops-pane ops-sync-rail" aria-label={copy.sync}>
      <PaneHead title={copy.sync} description={copy.syncDescription} />
      <div className="ops-signal-list">
        <Signal label={copy.accounts} value={String(data.accounts.length)} />
        <Signal label={copy.opportunities} value={String(data.opportunities.length)} />
        <Signal label={copy.contacts} value={String(data.contacts.length)} />
      </div>
      <div className="ops-card-stack">
        {riskyAccounts.map((account) => (
          <a
            className="ops-work-card priority-high"
            href={salesPanelHref(query, submoduleId, "account", account.id, "right")}
            key={account.id}
            {...recordContext(account.name, submoduleId, "account", account.id)}
          >
            <strong>{account.name}</strong>
            <small>{account.health} - {formatMoney(account.annualValue)}</small>
            <span>{account.nextStep.en}</span>
          </a>
        ))}
      </div>
    </section>
  );
}

function tasksForLeadAccount(data: SalesBundle, account: SalesAccount) {
  const accountContactIds = data.contacts.filter((contact) => contact.accountId === account.id).map((contact) => contact.id);
  const accountOpportunityIds = data.opportunities.filter((opportunity) => opportunity.accountId === account.id).map((opportunity) => opportunity.id);
  return data.tasks.filter((task) => {
    if (task.linkedResource === "account") return task.linkedRecordId === account.id;
    if (task.linkedResource === "contact") return accountContactIds.includes(task.linkedRecordId);
    if (task.linkedResource === "opportunity") return accountOpportunityIds.includes(task.linkedRecordId);
    return false;
  });
}

function buildLeadAccessFlow({
  account,
  contact,
  contacts,
  locale,
  offer,
  opportunity,
  tasks
}: {
  account: SalesAccount;
  contact?: SalesContact;
  contacts: SalesContact[];
  locale: SupportedLocale;
  offer?: SalesOffer;
  opportunity?: SalesOpportunity;
  tasks: SalesTask[];
}): LeadFlow {
  const hasDemoOrCall = tasks.some((task) => {
    const subject = task.subject.toLowerCase();
    return subject.includes("demo") || subject.includes("call") || subject.includes("mapping");
  });
  const hasSecurityOrCommercial = tasks.some((task) => {
    const subject = task.subject.toLowerCase();
    return subject.includes("security") || subject.includes("order") || subject.includes("commercial");
  });
  const hasChampion = contacts.some((item) => item.relationship === "Champion" || item.relationship === "Decision maker");
  const blocked = account.health === "Red";
  const plannedTaskDate = firstOpenTaskDate(tasks) ?? offer?.issuedAt ?? opportunity?.closeDate ?? "2026-05-03";
  const sourceDate = contact?.lastTouch ?? offer?.issuedAt ?? plannedTaskDate;
  const meetingDate = opportunity?.closeDate ?? addIsoDays(plannedTaskDate, 7);
  const time = {
    email: formatFlowDate(plannedTaskDate, locale),
    follow: formatFlowDate(addIsoDays(plannedTaskDate, 2), locale),
    meeting: formatFlowDate(meetingDate, locale),
    offer: formatFlowDate(offer?.issuedAt ?? opportunity?.closeDate ?? addIsoDays(plannedTaskDate, 10), locale),
    prep: formatFlowDate(addIsoDays(meetingDate, -1), locale),
    reply: formatFlowDate(addIsoDays(plannedTaskDate, 1), locale),
    research: formatFlowDate("2026-05-03", locale),
    source: formatFlowDate(sourceDate, locale),
    wait: formatFlowDate(addIsoDays(plannedTaskDate, 1), locale)
  };
  const stage = opportunity?.stage ?? "Qualify";
  const nextStep = text(account.nextStep, locale);
  const contactName = contact?.name ?? (locale === "de" ? "Primaerer Kontakt offen" : "Primary contact open");
  const opportunityName = opportunity?.name ?? (locale === "de" ? "Direkt angelegter Lead" : "Directly created lead");

  const nodes: LeadFlowNode[] = [
    {
      actor: "System",
      detail: locale === "de" ? `${opportunityName} aus ${opportunity?.source ?? "direktem Lead"} uebernommen.` : `${opportunityName} entered from ${opportunity?.source ?? "direct lead"}.`,
      evidence: locale === "de" ? `${stage} - ${formatMoney(opportunity?.value ?? account.annualValue, "EUR", locale)}` : `${stage} - ${formatMoney(opportunity?.value ?? account.annualValue, "EUR", locale)}`,
      id: "source",
      state: "done",
      time: time.source,
      title: locale === "de" ? "Lead gewonnen" : "Lead won",
      type: "source",
      x: 24,
      y: 178
    },
    {
      actor: "Agent",
      detail: locale === "de" ? `CTOX prueft ${contactName}, Buying Trigger, Einwaende und eine passende erste Ansprache.` : `CTOX checks ${contactName}, buying triggers, objections, and the right first touch.`,
      evidence: contacts.length ? contacts.map((item) => `${item.name}: ${item.relationship}`).join(" / ") : nextStep,
      id: "research",
      state: blocked ? "blocked" : hasSecurityOrCommercial || hasChampion ? "done" : "running",
      time: time.research,
      title: locale === "de" ? "Research fuer Ansprache" : "Research for outreach",
      type: "research",
      x: 260,
      y: 54
    },
    {
      actor: "User",
      detail: locale === "de" ? "Kurze, konkrete Mail mit Anlass, Hypothese und genau einem naechsten Schritt vorbereiten und senden." : "Prepare and send a short email with trigger, hypothesis, and one concrete next step.",
      evidence: locale === "de" ? `Anker: ${nextStep}` : `Anchor: ${nextStep}`,
      id: "email",
      state: offer ? "done" : stage === "Proposal" || stage === "Negotiation" ? "running" : "planned",
      time: time.email,
      title: locale === "de" ? "E-Mail vorbereiten / senden" : "Prepare / send email",
      type: "message",
      x: 260,
      y: 290
    },
    {
      actor: "Agent",
      detail: locale === "de" ? "Nicht alles modellieren: Antwortfenster beobachten und erst beim naechsten Signal verzweigen." : "Do not model everything: watch the reply window and branch only on the next signal.",
      evidence: locale === "de" ? "Trigger: Antwort oder keine Rueckmeldung nach 2 Werktagen." : "Trigger: reply or no response after 2 business days.",
      id: "wait",
      state: "planned",
      time: time.wait,
      title: locale === "de" ? "Warten auf Antwort" : "Wait for reply",
      type: "wait",
      x: 510,
      y: 178
    },
    {
      actor: "Agent",
      detail: locale === "de" ? "Wenn keine Antwort kommt, Follow-up mit neuem Beleg oder konkreterem Nutzen vorbereiten." : "If there is no reply, prepare a follow-up with new evidence or a sharper value point.",
      evidence: hasSecurityOrCommercial ? (tasks.find((task) => /security|order|commercial/i.test(task.subject))?.subject ?? nextStep) : nextStep,
      id: "follow-up",
      state: "planned",
      time: time.follow,
      title: locale === "de" ? "Keine Antwort: Follow-up-Mail" : "No reply: follow-up email",
      type: "message",
      x: 760,
      y: 54
    },
    {
      actor: "Agent",
      detail: locale === "de" ? "Wenn geantwortet wird, keine neue Story starten, sondern Terminfindung mit klarer Agenda vorschlagen." : "If they reply, do not start a new story; move into scheduling with a clear agenda.",
      evidence: locale === "de" ? `Zielperson: ${contactName}` : `Target contact: ${contactName}`,
      id: "reply-branch",
      state: hasDemoOrCall ? "running" : "planned",
      time: time.reply,
      title: locale === "de" ? "Antwort: Terminfindung" : "Reply: scheduling",
      type: "message",
      x: 760,
      y: 290
    },
    {
      actor: "User",
      detail: locale === "de" ? "Zeitfenster, Teilnehmer, Ziel und erwartetes Ergebnis festlegen." : "Set time windows, participants, objective, and expected outcome.",
      evidence: opportunity?.risks[0] ? text(opportunity.risks[0], locale) : nextStep,
      id: "meeting",
      state: blocked ? "blocked" : hasDemoOrCall ? "running" : "planned",
      time: time.meeting,
      title: locale === "de" ? "Termin planen" : "Plan meeting",
      type: "meeting",
      x: 1010,
      y: 210
    },
    {
      actor: "Agent",
      detail: locale === "de" ? "Terminbriefing, Fragen, Einwaende und Demo-Ausschnitt vorbereiten." : "Prepare meeting brief, questions, objections, and demo slice.",
      evidence: hasDemoOrCall ? tasks.find((task) => /demo|call|mapping/i.test(task.subject))?.subject ?? nextStep : (locale === "de" ? "Noch nicht gestartet." : "Not started yet."),
      id: "meeting-prep",
      state: hasDemoOrCall ? "running" : "planned",
      time: time.prep,
      title: locale === "de" ? "Termin vorbereiten" : "Prepare meeting",
      type: "demo",
      x: 1010,
      y: 345
    },
    {
      actor: "System",
      detail: locale === "de" ? "Nach belastbarem Termin oder geklaertem Scope kann in Offers ueberfuehrt werden." : "After a solid meeting or clarified scope, the lead can move into Offers.",
      evidence: offer ? `${offer.number} - ${offer.status}` : (locale === "de" ? "Noch kein Angebot" : "No offer yet"),
      id: "offer",
      state: offer ? "done" : stage === "Proposal" || stage === "Negotiation" ? "running" : "planned",
      time: time.offer,
      title: locale === "de" ? "Angebotsreife" : "Offer readiness",
      type: "offer",
      x: 1010,
      y: 54
    }
  ];

  const links: LeadFlowLink[] = [
    { from: "source", id: "source-research", state: "done", to: "research" },
    { from: "research", id: "research-email", state: nodes.find((node) => node.id === "email")?.state ?? "planned", to: "email" },
    { from: "email", id: "email-wait", state: "planned", to: "wait" },
    { from: "wait", id: "wait-follow", state: "planned", to: "follow-up" },
    { from: "wait", id: "wait-reply", state: "planned", to: "reply-branch" },
    { from: "reply-branch", id: "reply-meeting", state: nodes.find((node) => node.id === "meeting")?.state ?? "planned", to: "meeting" },
    { from: "meeting", id: "meeting-prep", state: nodes.find((node) => node.id === "meeting-prep")?.state ?? "planned", to: "meeting-prep" },
    { from: "meeting", id: "meeting-offer", state: nodes.find((node) => node.id === "offer")?.state ?? "planned", to: "offer" }
  ];

  return {
    links,
    nodes,
    patterns: [
      {
        label: locale === "de" ? "Mail -> Warten -> Signal" : "Email -> wait -> signal",
        signal: locale === "de" ? "Nur die naechste Antwortlogik modellieren, keine komplette Eventualitaeten-Matrix." : "Model only the next response logic, not a full eventuality matrix.",
        state: "planned"
      },
      {
        label: locale === "de" ? "Antwort -> Terminfindung" : "Reply -> scheduling",
        signal: hasChampion ? (locale === "de" ? "Kontakt ist belastbar genug fuer Terminversuch." : "Contact is strong enough for a scheduling attempt.") : (locale === "de" ? "Vor Terminfindung Kontaktrolle schaerfen." : "Clarify contact role before scheduling."),
        state: hasChampion ? "done" : "running"
      },
      {
        label: locale === "de" ? "Keine Antwort -> Follow-up" : "No reply -> follow-up",
        signal: hasSecurityOrCommercial ? (locale === "de" ? "Follow-up kann mit Beleg statt Erinnerung rausgehen." : "Follow-up can use evidence instead of just reminding.") : (locale === "de" ? "Follow-up braucht noch Beleg oder neuen Trigger." : "Follow-up still needs evidence or a new trigger."),
        state: hasSecurityOrCommercial ? "running" : "planned"
      }
    ]
  };
}

function firstOpenTaskDate(tasks: SalesTask[]) {
  return [...tasks]
    .filter((task) => task.status !== "Done")
    .sort((left, right) => left.due.localeCompare(right.due))[0]?.due;
}

function addIsoDays(date: string, days: number) {
  const value = new Date(`${date}T00:00:00.000Z`);
  if (Number.isNaN(value.getTime())) return date;
  value.setUTCDate(value.getUTCDate() + days);
  return value.toISOString().slice(0, 10);
}

function formatFlowDate(date: string, locale: SupportedLocale) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
  const [year, month, day] = date.split("-");
  return locale === "de" ? `${day}.${month}.${year}` : `${month}/${day}/${year}`;
}

function opportunityStageLabel(stage: SalesOpportunityStage | undefined, locale: SupportedLocale) {
  if (!stage) return locale === "de" ? "Neu" : "New";
  if (locale === "de") {
    const labels: Record<SalesOpportunityStage, string> = {
      Discover: "Discovery",
      Negotiation: "Verhandlung",
      Proposal: "Angebot",
      Qualify: "Qualifizierung",
      Won: "Gewonnen"
    };
    return labels[stage];
  }
  return stage;
}

function buildLeadActivityCards(data: SalesBundle, locale: SupportedLocale, submoduleId: string, query: QueryState) {
  const cards: Array<{
    account: string;
    href: string;
    id: string;
    meta: string;
    nextStep: string;
    panel: string;
    priority: "low" | "normal" | "high" | "urgent";
    recordId: string;
    stage: typeof leadActivityStages[number];
    title: string;
  }> = [];

  data.accounts.forEach((account) => {
    const contact = data.contacts.find((item) => item.accountId === account.id);
    const opportunity = data.opportunities.find((item) => item.accountId === account.id && item.stage !== "Won");
    const offer = data.offers.find((item) => item.accountId === account.id);
    const accountTasks = data.tasks.filter((task) => {
      if (task.linkedResource === "account") return task.linkedRecordId === account.id;
      if (task.linkedResource === "contact") return data.contacts.find((item) => item.id === task.linkedRecordId)?.accountId === account.id;
      if (task.linkedResource === "opportunity") return data.opportunities.find((item) => item.id === task.linkedRecordId)?.accountId === account.id;
      return false;
    });
    const stage: typeof leadActivityStages[number] = offer
      ? "Angebot"
      : opportunity?.stage === "Proposal" || opportunity?.stage === "Negotiation"
        ? "Schriftverkehr"
        : accountTasks.some((task) => task.subject.toLowerCase().includes("mapping") || task.subject.toLowerCase().includes("call"))
          ? "Meeting"
          : account.id.includes("northstar")
            ? "Produkttest"
            : "Produktdemo";

    cards.push({
      account: account.name,
      href: salesPanelHref(query, submoduleId, offer ? "offer" : "account", offer?.id ?? account.id, "right"),
      id: `activity-${account.id}`,
      meta: offer ? `${offer.number} · ${offer.status}` : `${contact?.name ?? "-"} · ${ownerName(data, account.ownerId)}`,
      nextStep: offer ? text(offer.nextStep, locale) : text(account.nextStep, locale),
      panel: offer ? "offer" : "account",
      priority: account.health === "Red" ? "high" : account.health === "Amber" ? "normal" : "low",
      recordId: offer?.id ?? account.id,
      stage,
      title: offer ? offer.title : `${account.name} Sales Aktivitaet`
    });
  });

  data.tasks.filter((task) => task.status !== "Done").slice(0, 4).forEach((task) => {
    cards.push({
      account: describeLinkedRecord(data, task),
      href: salesPanelHref(query, submoduleId, "task", task.id, "right"),
      id: `activity-task-${task.id}`,
      meta: `${task.due} · ${task.status}`,
      nextStep: text(task.nextStep, locale),
      panel: "task",
      priority: task.priority.toLowerCase() as "low" | "normal" | "high" | "urgent",
      recordId: task.id,
      stage: task.subject.toLowerCase().includes("security") || task.subject.toLowerCase().includes("order") ? "Schriftverkehr" : "Meeting",
      title: task.subject
    });
  });

  return cards;
}

function customerFromOffer(offer: SalesOffer, accountName?: string, contactName?: string, email?: string): SalesCustomer {
  return {
    id: `customer-${offer.accountId}`,
    name: accountName ?? offer.accountId,
    contactName: contactName ?? offer.contactId,
    email: email ?? "",
    segment: "Accepted offer",
    ownerId: "customer-success",
    source: "Accepted offer",
    offerId: offer.id,
    onboardingStatus: "Queued",
    summary: {
      en: `Customer created from accepted offer ${offer.number}.`,
      de: `Kunde aus angenommenem Angebot ${offer.number}.`
    },
    nextStep: offer.nextStep
  };
}

function PaneHead({
  children,
  description,
  title
}: {
  children?: ReactNode;
  description?: string;
  title: string;
}) {
  return (
    <div className="ops-pane-head">
      <div>
        <h2>{title}</h2>
        {description ? <p>{description}</p> : null}
      </div>
      {children ? <div className="ops-pane-actions">{children}</div> : null}
    </div>
  );
}

function DrawerHeader({
  title,
  query,
  submoduleId
}: {
  title: string;
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = salesCopy[locale];

  return (
    <div className="drawer-head">
      <strong>{title}</strong>
      <a href={baseHref(query, submoduleId)}>{copy.close}</a>
    </div>
  );
}

function Fact({ label, value }: { label: string; value?: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value ?? "-"}</dd>
    </div>
  );
}

function Signal({ href, label, value }: { href?: string; label: string; value: string }) {
  const content = (
    <>
      <span>{label}</span>
      <strong>{value}</strong>
    </>
  );
  const context = href ? contextFromHref(href, label) : {};
  return href ? <a className="ops-signal" href={href} {...context}>{content}</a> : <div className="ops-signal" {...context}>{content}</div>;
}

function salesPanelHref(query: QueryState, submoduleId: string, panel: string, recordId: string, drawer: "left-bottom" | "bottom" | "right") {
  if (query.panel === panel && query.recordId === recordId) {
    return baseHref(query, submoduleId);
  }

  const params = new URLSearchParams();
  params.set("panel", panel);
  params.set("recordId", recordId);
  params.set("drawer", drawer);
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  return `/app/sales/${submoduleId}?${params.toString()}`;
}

function baseHref(query: QueryState, submoduleId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const queryString = params.toString();
  return queryString ? `/app/sales/${submoduleId}?${queryString}` : `/app/sales/${submoduleId}`;
}

function salesSelectionHref(query: QueryState, submoduleId: string, recordId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("selectedId", recordId);
  return `/app/sales/${submoduleId}?${params.toString()}`;
}

function salesRecordHref(query: QueryState, submoduleId: string, panel: string, recordId: string) {
  return salesPanelHref(query, submoduleId, panel, recordId, "right");
}

function resolveNewResource(recordId: string | undefined, submoduleId: string) {
  if (recordId?.includes("account") || submoduleId === "accounts" || submoduleId === "leads") return "accounts";
  if (recordId?.includes("contact") || submoduleId === "contacts") return "contacts";
  if (recordId?.includes("lead")) return "leads";
  if (recordId?.includes("offer") || submoduleId === "offers") return "offers";
  if (recordId?.includes("customer") || submoduleId === "customers") return "customers";
  if (recordId?.includes("task") || submoduleId === "tasks") return "tasks";
  return "opportunities";
}

function recordContext(label: string, submoduleId: string, recordType: string, recordId: string) {
  return {
    "data-context-item": true,
    "data-context-label": label,
    "data-context-module": "sales",
    "data-context-record-id": recordId,
    "data-context-record-type": recordType,
    "data-context-submodule": submoduleId
  };
}

function createContext(label: string, submoduleId: string, recordType: string) {
  return {
    "data-context-action": "create",
    "data-context-item": true,
    "data-context-label": label,
    "data-context-module": "sales",
    "data-context-record-id": recordType,
    "data-context-record-type": recordType,
    "data-context-submodule": submoduleId
  };
}

function contextFromHref(href: string, label: string) {
  const [path, search = ""] = href.split("?");
  const [, moduleId = "sales", submoduleId = "pipeline"] = path.match(/\/app\/([^/]+)\/([^/?]+)/) ?? [];
  const params = new URLSearchParams(search);
  const panel = params.get("panel") ?? "record";
  const recordId = params.get("recordId") ?? label.toLowerCase().replaceAll(" ", "-");

  return {
    "data-context-action": panel.includes("set") ? "open-set" : "open",
    "data-context-item": true,
    "data-context-label": label,
    "data-context-module": moduleId,
    "data-context-record-id": recordId,
    "data-context-record-type": panel,
    "data-context-submodule": submoduleId
  };
}

function ownerName(data: SalesBundle, ownerId: string) {
  return data.owners.find((owner) => owner.id === ownerId)?.name ?? ownerId;
}

function describeLinkedRecord(data: SalesBundle, task: SalesTask) {
  if (task.linkedResource === "opportunity") return data.opportunities.find((item) => item.id === task.linkedRecordId)?.name ?? task.linkedRecordId;
  if (task.linkedResource === "account") return data.accounts.find((item) => item.id === task.linkedRecordId)?.name ?? task.linkedRecordId;
  if (task.linkedResource === "contact") return data.contacts.find((item) => item.id === task.linkedRecordId)?.name ?? task.linkedRecordId;
  return data.leads.find((item) => item.id === task.linkedRecordId)?.company ?? task.linkedRecordId;
}

function nextCloseDate(opportunities: SalesOpportunity[]) {
  return opportunities
    .filter((opportunity) => opportunity.stage !== "Won")
    .sort((left, right) => left.closeDate.localeCompare(right.closeDate))[0]?.closeDate ?? "-";
}

function expiringOfferCount(offers: SalesOffer[]) {
  return offers.filter((offer) => offer.status === "Sent" && offer.validUntil <= "2026-05-15").length;
}

type SalesSetItem = {
  id: string;
  label: string;
  meta: string;
  panel: string;
  type: string;
  value?: number;
};

function resolveSalesSet(recordId: string | undefined, data: SalesBundle, copy: Copy) {
  const key = recordId ?? "open-pipeline";
  const openOpportunities = data.opportunities.filter((opportunity) => opportunity.stage !== "Won");
  const opportunityItems = (items: SalesOpportunity[]): SalesSetItem[] => items.map((opportunity) => ({
    id: opportunity.id,
    label: opportunity.name,
    meta: `${opportunity.stage} · ${formatMoney(opportunity.value)} · ${opportunity.probability}%`,
    panel: "opportunity",
    type: "opportunity",
    value: opportunity.value
  }));
  const taskItems = (items: SalesTask[]): SalesSetItem[] => items.map((task) => ({
    id: task.id,
    label: task.subject,
    meta: `${task.priority} · ${task.status} · ${task.due}`,
    panel: "task",
    type: "task"
  }));
  const contactItems: SalesSetItem[] = data.contacts.map((contact) => ({
    id: contact.id,
    label: contact.name,
    meta: `${contact.relationship} · ${contact.role}`,
    panel: "contact",
    type: "contact"
  }));
  const accountItems: SalesSetItem[] = data.accounts.map((account) => ({
    id: account.id,
    label: account.name,
    meta: `${account.health} · ${account.segment} · ${formatMoney(account.annualValue)}`,
    panel: "account",
    type: "account",
    value: account.annualValue
  }));
  const offerItems = (items: SalesOffer[]): SalesSetItem[] => items.map((offer) => ({
    id: offer.id,
    label: `${offer.number} ${offer.title}`,
    meta: `${offer.status} · ${offer.validUntil} · ${formatMoney(offer.grossAmount, offer.currency)}`,
    panel: "offer",
    type: "offer",
    value: offer.grossAmount
  }));

  if (key === "weighted-forecast") {
    return {
      title: copy.weightedForecast,
      description: copy.salesSetForecastDescription,
      items: opportunityItems(openOpportunities),
      resource: "opportunities",
      value: Math.round(openOpportunities.reduce((sum, opportunity) => sum + (opportunity.value * opportunity.probability / 100), 0))
    };
  }

  if (key === "next-close") {
    const sorted = [...openOpportunities].sort((left, right) => left.closeDate.localeCompare(right.closeDate));
    const nextDate = sorted[0]?.closeDate;
    const items = opportunityItems(nextDate ? sorted.filter((opportunity) => opportunity.closeDate === nextDate) : []);
    return {
      title: copy.nextClose,
      description: copy.salesSetCloseDescription,
      items,
      resource: "opportunities",
      value: items.reduce((sum, item) => sum + (item.value ?? 0), 0)
    };
  }

  if (key === "champions" || key === "decision-makers") {
    const relationship = key === "champions" ? "Champion" : "Decision maker";
    const items = contactItems.filter((contact) => contact.meta.startsWith(relationship));
    return {
      title: key === "champions" ? copy.champions : copy.decisionMakers,
      description: copy.salesSetRelationshipDescription,
      items,
      resource: "contacts",
      value: 0
    };
  }

  if (key === "accounts") {
    return {
      title: copy.accounts,
      description: copy.salesSetAccountsDescription,
      items: accountItems,
      resource: "accounts",
      value: accountItems.reduce((sum, item) => sum + (item.value ?? 0), 0)
    };
  }

  if (key === "urgent-tasks" || key === "open-tasks" || key === "waiting-tasks") {
    const tasks = data.tasks.filter((task) => {
      if (key === "urgent-tasks") return task.priority === "Urgent";
      if (key === "waiting-tasks") return task.status === "Waiting";
      return task.status !== "Done";
    });
    return {
      title: key === "urgent-tasks" ? copy.urgent : key === "waiting-tasks" ? copy.waiting : copy.open,
      description: copy.salesSetTasksDescription,
      items: taskItems(tasks),
      resource: "tasks",
      value: 0
    };
  }

  if (key === "offers-open" || key === "offers-valid" || key === "offers-expiring" || key === "offers-accepted" || key === "offers-declined") {
    const offers = data.offers.filter((offer) => {
      if (key === "offers-valid") return offer.status === "Sent" || offer.status === "Draft";
      if (key === "offers-expiring") return offer.status === "Sent" && offer.validUntil <= "2026-05-15";
      if (key === "offers-accepted") return offer.status === "Accepted";
      if (key === "offers-declined") return offer.status === "Declined" || offer.status === "Expired";
      return offer.status === "Draft" || offer.status === "Sent";
    });
    const items = offerItems(offers);
    return {
      title: key === "offers-accepted" ? copy.acceptedOffers : key === "offers-expiring" ? copy.expiringOffers : key === "offers-declined" ? copy.declinedOffers : copy.openOffers,
      description: copy.salesSetOffersDescription,
      items,
      resource: "offers",
      value: items.reduce((sum, item) => sum + (item.value ?? 0), 0)
    };
  }

  const items = opportunityItems(openOpportunities);
  return {
    title: copy.openPipeline,
    description: copy.salesSetPipelineDescription,
    items,
    resource: "opportunities",
    value: items.reduce((sum, item) => sum + (item.value ?? 0), 0)
  };
}

function formatMoney(value: number, currency: "EUR" | "USD" = "EUR", locale: SupportedLocale = "en") {
  return new Intl.NumberFormat(locale === "de" ? "de-DE" : "en-US", {
    currency,
    maximumFractionDigits: 0,
    style: "currency"
  }).format(value);
}

function offerUnitLabel(unit: SalesOffer["lineItems"][number]["unit"], copy: Copy) {
  if (unit === "Hour") return copy.hour;
  if (unit === "Day") return copy.dayRate;
  if (unit === "Month") return copy.month;
  return copy.piece;
}

type SalesViewProps = {
  copy: Copy;
  data: SalesBundle;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
};

type Copy = typeof salesCopy.en;

const salesCopy = {
  en: {
    account: "Account",
    accounts: "Accounts",
    accountsDescription: "Customer and prospect companies with value, health, next step, and handoff context.",
    acceptedOffers: "Accepted offers",
    all: "All",
    activeLeads: "Active leads",
    activityStatus: "Activity status",
    amount: "Amount",
    askCtox: "Ask CTOX to sync this",
    askCtoxSet: "Ask CTOX to process this set",
    champions: "Champions",
    close: "Close",
    closeDate: "Close date",
    contact: "Contact",
    contacts: "Contacts",
    contactsDescription: "People, influence, relationship health, and the next conversational move.",
    convertToCustomer: "Create customer and onboarding",
    convertToInvoice: "Convert to invoice draft",
    createOnboardingProject: "Create onboarding project",
    createOnboardingProjects: "Create onboarding projects",
    createOfferFromLead: "Create offer from lead",
    customer: "Customer",
    customerHandoff: "Customer handoff",
    customerHandoffBoundary: "Sales ends here; onboarding and delivery continue in Operations.",
    customerHandoffDescription: "Accepted offers become customers and create onboarding projects for Operations.",
    customers: "Customers",
    customersDescription: "Standalone customers and customers converted from accepted offers, with optional Operations onboarding handoff.",
    decisionMakers: "Decision makers",
    declinedOffers: "Declined offers",
    directCustomerNoPrerequisite: "No prior campaign, pipeline, lead, or offer is required.",
    closingText: "Closing text",
    documentTitle: "Document title",
    due: "Due",
    edit: "edit",
    email: "Email",
    expiringOffers: "Expiring offers",
    gross: "Gross",
    handoffScope: "Handoff scope",
    health: "Health",
    inSalesActivity: "In Sales activity",
    introText: "Intro text",
    items: "items",
    lastTouch: "Last touch",
    lead: "Lead",
    leadHandoff: "Lead handoff",
    leadHandoffDescription: "Won pipeline leads move into structured Sales activity before an offer is created.",
    leadSignals: "Lead signals",
    leadSignalsDescription: "Highest-fit leads for CTOX research and qualification work.",
    leads: "Leads",
    leadsDescription: "Won pipeline leads with demos, product tests, meetings, correspondence, and offer readiness.",
    lines: "Lines",
    linkedRecord: "Linked record",
    moreDetails: "More details",
    net: "Net",
    newAccount: "New account",
    newContact: "New contact",
    newCustomer: "New customer",
    newLead: "New lead",
    newOffer: "New offer",
    newOpportunity: "New opportunity",
    newRecord: "New Sales record",
    newRecordDescription: "Queue a CRM mutation through CTOX so the generated app and core context stay aligned.",
    newTask: "New task",
    nextClose: "Next close",
    nextStep: "Next step",
    noPrerequisite: "No prerequisite",
    noRisks: "No active risks.",
    offer: "Offer",
    offerNumber: "Offer number",
    offerPreview: "Offer preview",
    offerPreviewDescription: "Selected document terms, line items, and Business handoff readiness.",
    offerStatus: "Offer status",
    offerStatusDescription: "Lexoffice-style document states from draft through accepted or expired.",
    offerText: "Offer text",
    offers: "Offers",
    offersDescription: "Commercial documents with validity, text blocks, line items, tax, and customer handoff.",
    onboarding: "Onboarding",
    onboardingProjects: "Onboarding projects",
    onboardingQueued: "Onboarding queued",
    open: "Open",
    openOffers: "Open offers",
    openPipeline: "Open pipeline",
    operationsTakesOver: "Operations takes over with a customer onboarding project.",
    opportunities: "Opportunities",
    opportunity: "Opportunity",
    owner: "Owner",
    paymentTerms: "Payment terms",
    phone: "Phone",
    pipeline: "Pipeline",
    pipelineBoard: "Stage board",
    pipelineBoardDescription: "Opportunities stay in a single working view; details open in the drawer.",
    pipelineDescription: "Forecast, close pressure, and immediate follow-up work.",
    priority: "Priority",
    probability: "Probability",
    preview: "Preview",
    positions: "Line items",
    pipelineSignals: "Pipeline signals",
    queueCreate: "Queue create",
    readyForOffer: "Ready for offer",
    region: "Region",
    relationship: "Relationship",
    relationships: "Relationships",
    relationshipsDescription: "Coverage across champions, decision makers, and account contacts.",
    renewal: "Renewal",
    risks: "Risks",
    role: "Role",
    score: "Score",
    searchOffers: "Search by customer, offer number, or amount",
    sendOffer: "Send offer",
    selectedItems: "Selected items",
    saveDraft: "Save draft",
    salesSetAccountsDescription: "All won leads as synchronized Sales activity context.",
    salesSetCloseDescription: "Opportunities at the next close date, ready for immediate follow-up.",
    salesSetForecastDescription: "Open opportunities weighted by probability for forecast review.",
    salesSetOffersDescription: "Offers grouped by document status, validity, and customer handoff readiness.",
    salesSetPipelineDescription: "All open opportunities in the active pipeline.",
    salesSetRelationshipDescription: "Key buyer relationships that should drive account planning and next actions.",
    salesSetTasksDescription: "Sales tasks grouped as an actionable follow-up set.",
    salesActivityHandoff: "Activities end in an offer; accepted offers become customers.",
    salesActivityPipeline: "Sales activity pipeline",
    salesActivityPipelineDescription: "A normalized activity path after a lead is won: demos, tests, meetings, correspondence, then offer.",
    segment: "Segment",
    source: "Source",
    stage: "Stage",
    status: "Status",
    sync: "Sync",
    tax: "Tax",
    syncDescription: "Sales records that should stay connected to Operations, Marketing, Business, and CTOX.",
    task: "Task",
    taskPressure: "Task pressure",
    taskPressureDescription: "Due work that should drive the next sales action.",
    tasks: "Tasks",
    tasksDescription: "Follow-ups, handoffs, research jobs, and forecast-maintenance work.",
    urgent: "Urgent",
    validOffers: "Valid offers",
    validUntil: "Valid until",
    value: "Value",
    waiting: "Waiting",
    weightedForecast: "Weighted forecast",
    address: "Address",
    addressExtra: "Address extra",
    addLineItem: "Line item",
    autoGenerated: "Auto generated",
    cancel: "Cancel",
    city: "City",
    company: "Company",
    companyName: "Company name",
    country: "Country",
    createAsCustomer: "Create as customer",
    createCustomer: "Create customer",
    customerNotFoundPrompt: "This customer is not in master data yet.",
    customerNumber: "Customer number",
    customerType: "Contact",
    date: "Date",
    dayRate: "Day rate",
    deleteLine: "Delete line",
    deliveryDate: "Delivery date",
    deliveryOrService: "Delivery / service",
    deliveryPeriod: "Delivery period",
    description: "Description",
    discount: "Discount",
    discountIn: "Discount in",
    duplicateLine: "Duplicate line",
    freeText: "Free text",
    hour: "Hour",
    invoiceNumber: "Offer number",
    issueDate: "Issue date",
    lineItem: "Line item",
    manageUnits: "Manage units",
    month: "Month",
    moveDown: "Move down",
    moveUp: "Move up",
    noServiceDate: "No service date",
    offerIntroTextPlaceholder: "Describe scope, value, and the concrete commercial offer.",
    paymentConditionPlaceholder: "Payment target, acceptance terms, billing contact, and references.",
    percent: "Percent",
    person: "Person",
    piece: "Piece",
    postalCode: "Postal code",
    quantity: "Quantity",
    save: "Save",
    selectCustomer: "Select customer",
    serviceDate: "Service date",
    servicePeriod: "Service period",
    street: "Street",
    subtotalNet: "Subtotal net",
    taxRate: "Tax rate",
    totalDiscount: "Total discount",
    totalGross: "Total gross",
    unit: "Unit",
    unitNet: "Unit net",
    useWithoutMasterData: "Use without master data",
    vat: "VAT",
    closingNotePlaceholder: "Thank the customer, confirm next steps, and describe acceptance or handoff.",
    wonPipelineLeads: "Won pipeline leads"
  },
  de: {
    account: "Account",
    accounts: "Accounts",
    accountsDescription: "Kunden und Prospects mit Wert, Zustand, naechstem Schritt und Handoff-Kontext.",
    acceptedOffers: "Angenommene Angebote",
    all: "Alle",
    activeLeads: "Aktive Leads",
    activityStatus: "Aktivitaetsstatus",
    amount: "Betrag",
    askCtox: "CTOX synchronisieren lassen",
    askCtoxSet: "CTOX mit dieser Auswahl beauftragen",
    champions: "Champions",
    close: "Schliessen",
    closeDate: "Abschlussdatum",
    contact: "Kontakt",
    contacts: "Kontakte",
    contactsDescription: "Personen, Einfluss, Beziehung und der naechste Gespraechsschritt.",
    convertToCustomer: "Kunde und Onboarding anlegen",
    convertToInvoice: "In Rechnungsentwurf ueberfuehren",
    createOnboardingProject: "Onboarding-Projekt anlegen",
    createOnboardingProjects: "Onboarding-Projekte anlegen",
    createOfferFromLead: "Angebot aus Lead anlegen",
    customer: "Kunde",
    customerHandoff: "Customer Handoff",
    customerHandoffBoundary: "Sales endet hier; Onboarding und Delivery laufen in Operations weiter.",
    customerHandoffDescription: "Angenommene Angebote werden Kunden und erzeugen Onboarding-Projekte fuer Operations.",
    customers: "Customers",
    customersDescription: "Direkt angelegte Kunden und Kunden aus angenommenen Angeboten mit optionalem Operations-Onboarding.",
    decisionMakers: "Entscheider",
    declinedOffers: "Abgelehnte Angebote",
    directCustomerNoPrerequisite: "Keine vorherige Kampagne, Pipeline, Lead oder Angebot erforderlich.",
    closingText: "Schlusstext",
    documentTitle: "Belegtitel",
    due: "Faellig",
    edit: "bearbeiten",
    email: "E-Mail",
    expiringOffers: "Bald ablaufend",
    gross: "Brutto",
    handoffScope: "Handoff-Scope",
    health: "Zustand",
    inSalesActivity: "In Sales-Aktivitaet",
    introText: "Einleitungstext",
    items: "Eintraege",
    lastTouch: "Letzter Kontakt",
    lead: "Lead",
    leadHandoff: "Lead Handoff",
    leadHandoffDescription: "Gewonnene Pipeline-Leads laufen durch strukturierte Sales-Aktivitaeten, bevor ein Angebot entsteht.",
    leadSignals: "Lead Signale",
    leadSignalsDescription: "Beste Leads fuer CTOX Research und Qualifizierung.",
    leads: "Leads",
    leadsDescription: "Gewonnene Pipeline-Leads mit Demos, Produkttests, Meetings, Schriftverkehr und Angebotsreife.",
    lines: "Positionen",
    linkedRecord: "Verknuepft",
    moreDetails: "Mehr Details",
    net: "Netto",
    newAccount: "Neuer Account",
    newContact: "Neuer Kontakt",
    newCustomer: "Neuer Kunde",
    newLead: "Neuer Lead",
    newOffer: "Neues Angebot",
    newOpportunity: "Neue Opportunity",
    newRecord: "Neuer Sales Datensatz",
    newRecordDescription: "CRM Mutation ueber CTOX queuen, damit App und Core-Kontext synchron bleiben.",
    newTask: "Neue Aufgabe",
    nextClose: "Naechster Abschluss",
    nextStep: "Naechster Schritt",
    noPrerequisite: "Keine Vorstufe",
    noRisks: "Keine aktiven Risiken.",
    offer: "Angebot",
    offerNumber: "Angebotsnummer",
    offerPreview: "Angebotsvorschau",
    offerPreviewDescription: "Ausgewaehlte Belegtexte, Positionen und Business-Handoff-Bereitschaft.",
    offerStatus: "Angebotsstatus",
    offerStatusDescription: "Belegstatus nach Lexoffice-Logik von Entwurf bis angenommen oder abgelaufen.",
    offerText: "Angebotstext",
    offers: "Angebote",
    offersDescription: "Kommerzielle Belege mit Gueltigkeit, Textbausteinen, Positionen, Steuer und Customer-Handoff.",
    onboarding: "Onboarding",
    onboardingProjects: "Onboarding-Projekte",
    onboardingQueued: "Onboarding gequeued",
    open: "Offen",
    openOffers: "Offene Angebote",
    openPipeline: "Offene Pipeline",
    operationsTakesOver: "Operations uebernimmt mit einem Customer-Onboarding-Projekt.",
    opportunities: "Opportunities",
    opportunity: "Opportunity",
    owner: "Owner",
    paymentTerms: "Zahlungsbedingungen",
    phone: "Telefon",
    pipeline: "Pipeline",
    pipelineBoard: "Stage Board",
    pipelineBoardDescription: "Opportunities bleiben in einer Arbeitsansicht; Details oeffnen im Drawer.",
    pipelineDescription: "Forecast, Abschlussdruck und unmittelbare Follow-up Arbeit.",
    priority: "Prioritaet",
    probability: "Wahrscheinlichkeit",
    preview: "Vorschau",
    positions: "Positionen",
    pipelineSignals: "Pipeline-Signale",
    queueCreate: "Create queuen",
    readyForOffer: "Angebotsbereit",
    region: "Region",
    relationship: "Beziehung",
    relationships: "Beziehungen",
    relationshipsDescription: "Abdeckung ueber Champions, Entscheider und Account-Kontakte.",
    renewal: "Renewal",
    risks: "Risiken",
    role: "Rolle",
    score: "Score",
    searchOffers: "Nach Kunde, Angebotsnummer oder Betrag suchen",
    sendOffer: "Angebot senden",
    selectedItems: "Ausgewaehlte Eintraege",
    saveDraft: "Entwurf speichern",
    salesSetAccountsDescription: "Alle gewonnenen Leads als synchronisierter Sales-Aktivitaetskontext.",
    salesSetCloseDescription: "Opportunities mit dem naechsten Abschlussdatum fuer direkte Nacharbeit.",
    salesSetForecastDescription: "Offene Opportunities nach Wahrscheinlichkeit gewichtet fuer Forecast Review.",
    salesSetOffersDescription: "Angebote gruppiert nach Belegstatus, Gueltigkeit und Customer-Handoff.",
    salesSetPipelineDescription: "Alle offenen Opportunities in der aktiven Pipeline.",
    salesSetRelationshipDescription: "Zentrale Buyer-Beziehungen fuer Account Planning und naechste Aktionen.",
    salesSetTasksDescription: "Sales-Aufgaben als ausfuehrbare Follow-up-Auswahl.",
    salesActivityHandoff: "Aktivitaeten enden im Angebot; angenommene Angebote werden Customers.",
    salesActivityPipeline: "Sales-Aktivitaeten",
    salesActivityPipelineDescription: "Genormter Aktivitaetspfad nach gewonnenem Lead: Demo, Test, Meeting, Schriftverkehr, dann Angebot.",
    segment: "Segment",
    source: "Quelle",
    stage: "Stage",
    status: "Status",
    sync: "Sync",
    tax: "Steuer",
    syncDescription: "Sales-Datensaetze, die mit Operations, Marketing, Business und CTOX verbunden bleiben muessen.",
    task: "Aufgabe",
    taskPressure: "Aufgabendruck",
    taskPressureDescription: "Faellige Arbeit, die die naechste Sales-Aktion treiben sollte.",
    tasks: "Aufgaben",
    tasksDescription: "Follow-ups, Handoffs, Research Jobs und Forecast-Pflege.",
    urgent: "Dringend",
    validOffers: "Gueltige Angebote",
    validUntil: "Gueltig bis",
    value: "Wert",
    waiting: "Wartet",
    weightedForecast: "Gewichteter Forecast",
    address: "Adresse",
    addressExtra: "Adresszusatz",
    addLineItem: "Artikel",
    autoGenerated: "Automatisch",
    cancel: "Abbrechen",
    city: "Ort",
    company: "Unternehmen",
    companyName: "Unternehmensname",
    country: "Land",
    createAsCustomer: "Als Kunde anlegen",
    createCustomer: "Kunde anlegen",
    customerNotFoundPrompt: "Dieser Kunde ist noch nicht in den Stammdaten.",
    customerNumber: "Kundennummer",
    customerType: "Kontakt",
    date: "Datum",
    dayRate: "Tagessatz",
    deleteLine: "Position loeschen",
    deliveryDate: "Lieferdatum",
    deliveryOrService: "Lieferung / Leistung",
    deliveryPeriod: "Lieferzeitraum",
    description: "Beschreibung",
    discount: "Rabatt",
    discountIn: "Rabatt in",
    duplicateLine: "Position duplizieren",
    freeText: "Freitext",
    hour: "Stunde",
    invoiceNumber: "Angebotsnummer",
    issueDate: "Belegdatum",
    lineItem: "Position",
    manageUnits: "Einheiten verwalten",
    month: "Monat",
    moveDown: "Nach unten",
    moveUp: "Nach oben",
    noServiceDate: "Kein Leistungsdatum",
    offerIntroTextPlaceholder: "Scope, Nutzen und konkretes kommerzielles Angebot beschreiben.",
    paymentConditionPlaceholder: "Zahlungsziel, Annahmebedingungen, Rechnungskontakt und Referenzen.",
    percent: "Prozent",
    person: "Person",
    piece: "Stueck",
    postalCode: "PLZ",
    quantity: "Menge",
    save: "Speichern",
    selectCustomer: "Kunde auswaehlen",
    serviceDate: "Leistungsdatum",
    servicePeriod: "Leistungszeitraum",
    street: "Strasse",
    subtotalNet: "Zwischensumme netto",
    taxRate: "Steuersatz",
    totalDiscount: "Gesamtrabatt",
    totalGross: "Gesamt brutto",
    unit: "Einheit",
    unitNet: "Einzel netto",
    useWithoutMasterData: "Ohne Stammdaten nutzen",
    vat: "USt.",
    closingNotePlaceholder: "Dank, naechste Schritte und Annahme- oder Handoff-Hinweis.",
    wonPipelineLeads: "Gewonnene Pipeline-Leads"
  }
} satisfies Record<SupportedLocale, Record<string, string>>;
