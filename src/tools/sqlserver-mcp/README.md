# CTOX SQL Server Adapter

Generic SQL Server transport for CTOX external-data synchronization.

The crate contains connection handling, parameterized reads and writes,
transactions, metadata discovery, row conversion, and a restricted MCP
surface. It intentionally contains no customer schema, table mapping, module
name, or business workflow. Those belong to local Business OS app manifests.

Production credentials are resolved by CTOX from the secret store before a
connection is created. The standalone MCP binary accepts a config file for
diagnostics and requires `allowWrites: true` before executing a registered
write statement.
