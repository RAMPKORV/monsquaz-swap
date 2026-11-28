"""Universal smoke test: the project tracker answers and the database is readable."""
import json
import sqlite3
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools" / "project"))

import core  # noqa: E402


class TrackerSmokeTest(unittest.TestCase):
    def test_status_is_machine_readable(self):
        proc = subprocess.run(
            [sys.executable, str(ROOT / "tools/project/cli.py"), "status", "--json"],
            cwd=str(ROOT), text=True, capture_output=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        data = json.loads(proc.stdout)
        self.assertIn("prd", data)
        self.assertIn("counts", data)


class ReviewGateTest(unittest.TestCase):
    """The PRD force waiver must not waive the review gate (D-021, D-038)."""

    def setUp(self):
        self.con = sqlite3.connect(":memory:")
        self.con.row_factory = sqlite3.Row
        core.apply_schema(self.con)
        self.con.execute(
            """INSERT INTO issue(key,title,status_id,ceremony,details_json)
               VALUES('MSW-999','Review gate',(SELECT id FROM status WHERE key='merging'),
                      'standard','{}')"""
        )
        self.con.commit()
        self.issue = core.get_issue(self.con, "MSW-999")

    def tearDown(self):
        self.con.close()

    def test_prd_force_does_not_waive_the_review(self):
        with self.assertRaisesRegex(core.ProjectError, "resolved review"):
            core.assert_done_allowed(
                self.con,
                self.issue,
                evidence="abc123",
                force=True,
                require_merge=False,
                waive_review=False,
            )

    def test_generic_force_can_waive_the_review(self):
        core.assert_done_allowed(
            self.con,
            self.issue,
            evidence="abc123",
            force=True,
            require_merge=False,
        )


if __name__ == "__main__":
    unittest.main()
