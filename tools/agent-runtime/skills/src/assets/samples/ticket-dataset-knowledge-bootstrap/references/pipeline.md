# Ticket Dataset Knowledge Pipeline

This reference describes the concrete exemplar pipeline for building a ticket knowledge base from a large service-desk dataset.

## 1. Source Profile

The source profile should answer:

- what one row represents
- which fields are durable identifiers
- which fields are workflow/state controls
- which fields are categories or routing hints
- which fields are long-form narrative
- which fields likely reveal services, systems, teams, or sites

## 2. Promoted Structured Taxonomies

These usually come from strong categorical fields:

- channel family
- lifecycle state
- impact scope
- request type
- category tree
- reporter/system family

Only promote them when they are semantically coherent and repeatedly populated.

## 3. Semantic Issue-Pattern Taxonomy

Use embeddings over a compact row text built from fields such as:

- short description
- subcategory
- category
- reporter/system
- request excerpt
- action excerpt

Then:

- cluster similar rows
- select representative examples near cluster centers
- use the small LLM to name the cluster
- reject mixed or incoherent clusters

This is where repeated issue patterns become reusable knowledge instead of just ticket text.

## 4. Example Selection

For each promoted bucket:

- choose 1 to 2 canonical examples
- choose 2 to 4 common examples
- choose 1 edge example when boundaries are fuzzy

Examples should carry:

- ticket id
- brief title or short description
- why it belongs

## 5. Glossary

A useful ticket knowledge base should also produce a glossary from:

- structured category values
- repeated uppercase or system-like tokens
- repeated platform names from descriptions
- cluster naming outputs

The glossary is not just a word list.
It should be cleaned into stable terms with short meanings or usage notes.

## 6. Projection Guidance

The final knowledge base should say which parts feed later ticket onboarding:

- issue-pattern taxonomy
- service family taxonomy
- ownership hints
- access or monitoring gaps

That projection guidance is what turns the dataset analysis into actual CTOX onboarding value.
