# Doctor status fixture

`api-status-ready.json` is a reduced example of the gateway status response. Its field names and nesting follow the pinned Hermes status route. Every value is fabricated for the test: it represents current configuration, a healthy gateway, no active agents, and no configured platforms. It is not a recording of a gateway.

The three path fields use `TEMP_HOME_PLACEHOLDER` in place of a real home. The test server replaces that marker with its temporary directory before serving the response, so output can be checked for accidental path disclosure. The optional storage reason is absent in this healthy example.
