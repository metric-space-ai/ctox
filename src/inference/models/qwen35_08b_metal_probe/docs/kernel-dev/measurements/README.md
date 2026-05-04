# Measurement Records

`tools/new_measurement_record.sh` writes measurement records here.

Measurement records link captured run directories from
`tools/capture_measurement_output.sh` to experiments. They do not copy large raw
stdout/stderr payloads into docs; they validate the referenced files instead.
