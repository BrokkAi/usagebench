import unittest
import tempfile
import shutil
import sys
from pathlib import Path

# Ensure the parent directory is in sys.path
parent_dir = str(Path(__file__).parent.parent)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from analyzer import analyze

class TestFqnConsistency(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.root = Path(self.test_dir)
        
    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_fqn_consistency(self):
        # Create nested.py
        (self.root / "nested.py").write_text("""
class Outer:
    class Inner:
        def method(self): pass

def func():
    class Local:
        pass
    def inner_func():
        pass
""")
        
        # Create pkg/__init__.py
        pkg_dir = self.root / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("""
class InitClass: pass
""")

        usages = analyze(self.root)
        fqns = {u.fullyQualifiedName for u in usages.codeUnits}

        expected_fqns = [
            "nested.Outer",
            "nested.Outer$Inner",
            "nested.Outer$Inner.method",
            "nested.func",
            "nested.func$Local",
            "nested.func.inner_func",
            "pkg.InitClass"
        ]

        for expected in expected_fqns:
            with self.subTest(fqn=expected):
                self.assertIn(expected, fqns, f"FQN {expected} not found in analyzed units: {fqns}")

if __name__ == '__main__':
    unittest.main()
