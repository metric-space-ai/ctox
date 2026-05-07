# CTOX Business OS Product Context

register: product

## Product Purpose

CTOX Business OS is a self-hostable Next.js and Postgres business operating system template. CTOX installs it as a vanilla stack, then users adapt the entire system through CTOX prompts, bug reports, queue tasks, and module-level customization without overwriting their custom work during core upgrades.

The template is not a marketing website. It is a working app shell for real business work: sales, marketing, operations, business administration, and CTOX control surfaces.

## Users

- Founders and operators who need one integrated workspace instead of disconnected SaaS tools.
- CTOX agents that need stable URLs, record ids, module context, and queue-task hooks.
- Business users who expect direct manipulation, right-click prompts, bug reporting, language switching, and dark/light themes.

## Primary Jobs

- Show each module as a one-screen workbench with a useful default view.
- Keep all submodule workflows reachable in at most two navigation clicks: module, then submodule.
- Let users open contextual drawers or bottom panels for details, editing, prompts, and bug reports without navigating away.
- Preserve CTOX context for every record, selected item, and promptable surface.
- Support realistic starter data without leaking customer-specific secrets.

## Modules

- Sales: campaigns, pipeline, leads, offers, customers.
- Marketing: website wiring, assets, research, competitive analysis, commerce hooks.
- Operations: projects, work items, boards, planning, knowledge, meetings.
- Business: invoices, quotes, customers, ERP-style administration.
- CTOX: queue tasks, bug reports, prompts, links, locale, and core integration.

## Tone

Quiet, dense, operating-system-like, and utilitarian. The interface should feel like a serious tool that can run daily business work, not like a landing page or a generic SaaS demo.

## Anti-References

- Hero sections inside the application.
- Dashboard card mosaics that bury the actual work.
- Repeated generic buttons with unclear ownership.
- Decorative side stripes, glass panels, gradient noise, oversized metrics, and marketing copy.
- Navigating to nested detail pages when a drawer or inline work surface would preserve context.

## Strategic Principles

- The primary workspace always wins over summary chrome.
- Actions should be contextual, sparse, and named by their outcome.
- CTOX prompts and bug reports must carry precise module, submodule, record, and selection context.
- System design is global by default, with module-specific overrides only where the workflow needs them.
- Business automation must be visible as work state: queued, running, waiting for user, failed, complete.
