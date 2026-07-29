# Configuration: Parsing, Merging, and Serialization

StarForge's configuration lives in `~/.config/starforge/` (a local SQLite store,
with `config.toml` still read for backwards compatibility). This document
covers the parts a developer or CI author needs: the pure API, the overlay
merge rules, and what makes a configuration invalid.

---

## Pure API

Everything in this list is a pure function — no filesystem, no database, no
environment. That is what makes configuration handling testable and what keeps
`cargo test` from mutating the developer's real config.

| Function | Purpose |
|---|---|
| `config::parse_config_str(&str)` | Parse a configuration from TOML |
| `config::parse_config_json(&str)` | Parse a configuration from JSON |
| `config::to_toml_string(&Config)` | Serialize to TOML |
| `config::to_json_string(&Config)` | Serialize to JSON |
| `config::parse_overlay_str(&str)` | Parse a partial overlay from TOML |
| `config::merge_configs(base, overlay)` | Layer an overlay onto a base and validate |
| `config::validate_config(&Config)` | Validate a whole configuration |
| `config::validate_network_exists(&Config, &str)` | Check a network reference against *that* config |

Round trips are exact in both formats: parsing what was serialized yields an
equal `Config`, and crossing formats (TOML → JSON → TOML) is stable. This is
enforced by [`tests/config_property_tests.rs`](../tests/config_property_tests.rs)
over hundreds of generated configurations.

> **Field order matters in `Config`.** TOML requires a table's scalar values to
> be emitted before its sub-tables. All scalars are declared first, then tables,
> then arrays of tables. Moving a scalar below a table makes `to_toml_string`
> fail at runtime. Deserialization is by key, so the order can change without
> breaking existing files.

---

## Overlays

A `ConfigOverlay` is a partial configuration layered on top of a base — for a
project-local file, a CI environment, or a named profile.

```toml
# overlay.toml
network = "mainnet"
telemetry_enabled = false

[networks.staging]
horizon_url = "https://horizon-staging.example.com"
soroban_rpc_url = "https://rpc-staging.example.com"
```

### Precedence

| Field | Rule |
|---|---|
| `network`, `telemetry_enabled`, `wallet_encryption` | Overlay wins when set; base kept otherwise |
| `feature_flags`, `plugin_trust`, `ai_telemetry` | Replaced **wholesale** when present |
| `networks` | Merged by key; an overlay entry replaces the base entry of that name |
| `wallets` | Appended; a duplicate name is an error |
| `version`, `install_id` | Always from the base — an overlay may not forge either |

`feature_flags`, `plugin_trust`, and `ai_telemetry` are replaced rather than
field-merged on purpose: a partially specified trust policy that silently
inherits half of the base allowlist is a security footgun.

Wallets are appended rather than overwritten because they hold key material. An
overlay whose wallet name collides with the base is rejected:

```
Overlay wallet 'deployer' already exists in the base configuration; rename it
or remove it from the overlay
```

### Properties

Guaranteed, and tested over generated inputs:

- **Identity** — merging an empty overlay changes nothing.
- **Idempotence** — applying the same overlay twice adds nothing the first
  application did not already add.
- **Validation** — `merge_configs` validates the result, so a merge can never
  produce a configuration that `save` would reject.

### Unknown keys are rejected

`ConfigOverlay` uses `deny_unknown_fields`. A typo is a hard error:

```
$ starforge ... # with `netwrok = "mainnet"` in the overlay
Failed to parse configuration overlay TOML: unknown field `netwrok`
```

A silently ignored `netwrok` key would leave a deploy pointed at the wrong
network. The main `Config` parser stays lenient about unknown keys so a file
written by a newer StarForge still loads.

---

## What makes a configuration invalid

`validate_config` rejects these *combinations* — each value may be well-formed
on its own:

| Rejected | Why |
|---|---|
| Empty `version` | Schema version is required for migration |
| Empty or whitespace `network` | No active network |
| Active network not in `networks` and not built in | Dangling reference |
| Empty `networks` map | Nothing to connect to |
| A non-`http(s)` endpoint URL | Only HTTP(S) endpoints are supported |
| A wallet on an unknown network | Dangling reference |
| An invalid wallet name, public key, or secret key | Malformed entry |
| Two wallets with the same name | Ambiguous reference |
| An invalid plugin trust source | Malformed allowlist entry |

Built-in networks (`testnet`, `mainnet`, `docker-testnet`) always resolve, even
if they are absent from the `networks` map.

---

## Migration note

`validate_network_exists` used to fall back to loading the on-disk
configuration when a network was missing from the `Config` it was handed. That
made validation depend on — and, through `load()`, *write to* — global state:
the same in-memory `Config` could validate differently on two machines, and
validating a value in a test opened and migrated the developer's real database.

It is now pure: it consults the supplied `Config` and the built-in names, and
nothing else.

**If you relied on the old behaviour** (calling `validate_network_exists` with a
partially populated `Config` and expecting it to consult the saved config),
load the configuration explicitly first and pass that in. `validate_network`,
which does read from disk by design, is unchanged.

`validate_config` also now rejects duplicate wallet names. A configuration that
already contains duplicates will fail to save until one is renamed — previously
the duplicate silently shadowed the other on lookup.

---

## Security

- Wallet secrets in a configuration are stored either as plaintext StrKeys or
  as encrypted bundles; `validate_config` accepts both shapes but never logs
  either. Error messages quote the wallet name, not the key.
- An overlay cannot replace an existing wallet, so a hostile overlay file
  cannot swap out a deployer key.
- An overlay cannot set `install_id`, which is used for deterministic
  feature-flag bucketing.

---

## See also

- [docs/COMMAND_REFERENCE.md](COMMAND_REFERENCE.md) — the `config` command
- [FUZZING_GUIDE.md](../FUZZING_GUIDE.md) — running the property suites
- [tests/config_property_tests.rs](../tests/config_property_tests.rs)
