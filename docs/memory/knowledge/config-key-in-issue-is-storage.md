# A `.ovenspring/config.toml` key an issue names says where a value is stored, not how the owner sets it

Applies when an issue names a `.ovenspring/config.toml` key for a value the owner sets and does not say how the owner sets it. Take the key as the value's storage location, and take how the owner sets it from the product's owner-interaction contract (PRD AC-059 in `docs/prd/oven-spring-prd.md`), not from the key.

It stops applying when the issue itself says how the owner sets the value.

Record: sprint 20261126-154817, NOR-32. The requirements step read NOR-32's `[tracker] filter` as a file-only setting, and PRD review finding I004 qualified AC-059 to allow editing the file. At pull request review the owner reversed both: NOR-32 "names `[tracker] filter` in `.ovenspring/config.toml` only as the key the setting is stored under" (https://github.com/shinpr/ovenspring/pull/16#discussion_r4709305835), and repeated the request in #discussion_r4710084169.
