---
name: project-init
description: Onboard a newly bound Workjet project autonomously in one Supervisor turn - analyse the repository, create three project KPIs, hand over to the exit evaluation and start the first parent worker.
---

# ProjectInit

You onboard a project that the Owner (Michael) just added with one click. Work autonomously; never ask the Owner a question in this turn. Use the native project and its registered Supervisor binding; names/UUIDs grant no authority. No deployment, external message, purchase, grant change or edits outside this project.

Native onboarding already set the weekly Regeltermin (Jour fixe, Monday 09:00 Europe/Berlin unless the Owner chose one) and queued the 5-year exit evaluation as a separate research turn. Do not reschedule either.

Steps, in this order. Each step is idempotent: reuse the operation_id given in the brief plus the step suffix, and read state before writing.
1. analyse: read the bound repository or folder - README, stack, build/deploy files, website, releases, open issues and PRs. Summarise purpose, audience, revenue model and current stage in three German sentences for the speaker notes of the first Jour fixe.
2. kpis: read business_os.project_kpi {action:"read",request:{project_id}}. If the project has no KPI prompts, call {action:"initialize",request:{operation_id:"<id>-kpis",project_id,expected_revision:0,prompts:[3 items]}}. Each prompt is one German sentence naming a measurable, project-specific quantity with its window (for example "Gemergte Pull Requests der letzten 7 Tage", "Aktive Nutzer der letzten 30 Tage"). Prefer quantities the registered recipe catalogue can calculate; never invent values. Then bind every prompt that matches a recipe with action "resolve". Prompts without a source stay missing_source; that is a graceful outcome, not a failure.
3. exit: do not compute or state a valuation. The exit research turn returns its own inputs; if evidence is missing it stays blocked and becomes an open point for the first Jour fixe.
4. worker: start the first parent worker via business_os.workjet_worker_dispatch with one concrete, verifiable goal derived from the analysis (the most valuable next deliverable, with acceptance criteria). Read observe afterwards; startup is not completion.
5. report: finish with one line per step in the form "<step>: done|skipped|blocked - <short reason>" for steps analyse, kpis, exit, regeltermin, worker. Blocked steps name the missing prerequisite (for example a model account or a missing repository permission) and continue with the next step.
