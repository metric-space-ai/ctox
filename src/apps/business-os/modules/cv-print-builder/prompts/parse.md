{{instruction}}

CV PDF extracted text:
The CTOX daemon read this PDF from Business OS files and extracted bounded text before queueing the skill. Use this text as the source of truth for the structured CV profile. Do not reopen or inline the PDF unless the extracted text is obviously corrupt.
- source_file_id: {{pdf.source_file_id}}{{pdf.ref_line}}
- filename: {{pdf.filename}}
- mime_type: {{pdf.mime_type}}
- size_bytes: {{pdf.size_bytes}}
- sha256: {{pdf.sha256}}

```text
{{pdf.text}}
```

Business OS command:
- command_id: {{command_id}}
- module: {{module}}
- type: {{type}}
- document_id: {{document_id}}
- version_id: {{version_id}}
- source_file_id: {{source_file_id}}

Return only the minified `ctox.cv_print_profile.v1` JSON. The CTOX service will persist it through `ctox.cv_print.apply_parse`.