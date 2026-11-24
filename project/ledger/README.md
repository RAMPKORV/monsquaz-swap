# Project ledger

`current.jsonl` is the canonical, diffable export of `project.db`. It is committed so a
fresh clone can rebuild the database without binary merge conflicts.

- `current.jsonl` — one JSON object per row, deterministic table/row order.
- `current.jsonl.sha256` — content hash used by `project export --check`.
- `project.db` itself lives at `<workspace>/.project/project.db` and is **never committed**.

Rebuild:

```bash
python3 tools/project/cli.py rebuild --yes
```

Export after any DB mutation that should be recorded in git:

```bash
python3 tools/project/cli.py export
```

Rules:

- Do not hand-edit the dump. Fix the DB via the CLI and re-export.
- The dump is the rebuild source, not the operating source. Agents query the DB.
- If the dump and DB disagree, the DB is current until you export; CI uses the dump, so
  export before merging.
