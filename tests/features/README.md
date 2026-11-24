# Acceptance features

`*.feature` files here are **generated** from the `scenario` table in `project.db`.
Do not edit them by hand.

- Add or change a scenario: `python3 tools/project/cli.py prd scenario ...`
- Regenerate: `python3 tools/project/cli.py export`
- Check drift: `python3 tools/project/cli.py export --check`

Each file is named `<REQ-KEY>.feature` and tagged with the requirement and scenario IDs.
The test runner should map those tags back to `test_result` rows.
