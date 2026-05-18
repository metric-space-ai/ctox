# TechMB Dataset Notes

Source: https://huggingface.co/datasets/WSKL/techmb

TechMB is the Technical drawing for Manufacturability Benchmark. As of 2026-05-11, the Hugging Face dataset card describes:

- 947 question-answer pairs.
- 180 distinct technical drawings.
- Visual Question Answering task aimed at Vision Language Models.
- English and German content.
- CSV/parquet-style fields: `task_id`, `eval_type`, `drw_id`, `image`, `drw_complexity`, `question`, `answer`, `label_confidence`.
- CC-BY-4.0 license.

Use it for:

- Evaluating whether a VLM can read drawing metadata, material specs, dimensions, and drawing content.
- Creating benchmark prompts for drawing comprehension.
- Building regression tests for question-answer accuracy.
- Seeding example drawings for demos, provided attribution is preserved.

Do not treat it as direct training data for pinned issue detection because the public schema does not provide:

- Problem/defect annotations.
- Bounding boxes or pin coordinates.
- Reviewer comments tied to drawing regions.
- Confirmed issue severity or remediation actions.

Bridge strategy for a ClearHandoff-style mockup:

1. Use TechMB drawings as visual examples.
2. Ask the model to perform first-pass review and emit pinned findings with the JSON schema.
3. Mark the generated pins as synthetic review labels.
4. Have a human engineer audit a sample before using them as ground truth.
5. For evaluation, keep two tracks separate: VQA accuracy on TechMB labels and pinned-review quality on human-reviewed samples.

Attribution:

- Cite the dataset DOI from the dataset card when using it externally.
- Preserve CC-BY-4.0 attribution in demos, reports, or derived datasets.
