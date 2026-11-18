# Date and timestamp compatibility contract

`chrono-era-v1.json` is the `datetime-compat-v1` contract. Its SHA-256 is
`3c98d1bd98d2ed0e356004689d96cd73404813ef5b5933be71d5c2cfa3a3a50e`.

The producer baseline is mojentic commit
`896df2be74b724876af5b7a95bec1738b031b02f`, with chrono 0.4.45. The fixture
was generated with that implementation before removing chrono: fixed-offset
formatting and relative dates use chrono, and the tracer record uses the
public `LlmCallTracerEvent` serializer. Additional format cases were generated
against the same compiled chrono artifact. No live clock or service supplies
the fixture values.

The jiff consumer check runs through the public `LlmTool::run` interface with
an internal task-local fixed clock and deserializes/reserializes the original
tracer record. It covers leap-day date resolution, default and custom formats,
RFC 3339 offsets and 0/3/6/9 fractional digits, pre-epoch time, POSIX locale
directives, numeric timezone names, and optional tracer fields. Invalid formatting and out-of-range relative dates return errors.
Run it with `cargo test --all-features chrono_era_v1_payloads_remain_compatible`;
this is also part of the normal test and coverage gates.

Producer fixture generation passed at the baseline above. Consumer verification
passed in the 3.0.2 working tree based on that commit; Foundry assigns the final
commit. The existing public recovery suite separately verifies numeric and all
three HTTP Retry-After date forms, errors and the moving RFC 850 century pivot.
This change replaces an internal dependency; it does not change an application
or repository protocol or require changes in another authorized repository.
