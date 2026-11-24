"""Universal smoke test: the project tracker answers and the database is readable."""
import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


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


if __name__ == "__main__":
    unittest.main()
