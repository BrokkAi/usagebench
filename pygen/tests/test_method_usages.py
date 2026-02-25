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

class TestMethodUsages(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.root = Path(self.test_dir)
        
    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_method_usages(self):
        # Create methods.py
        (self.root / "methods.py").write_text("""
class MyClass:
    def instance_method(self):
        pass
    
    @staticmethod
    def static_method():
        pass
""")
        
        # Create caller.py
        (self.root / "caller.py").write_text("""
from methods import MyClass

def main():
    obj = MyClass()
    obj.instance_method()
    MyClass.static_method()
""")

        usages = analyze(self.root)
        
        # Verify instance_method
        inst_method = next((u for u in usages.codeUnits if u.fullyQualifiedName == "methods.MyClass.instance_method"), None)
        self.assertIsNotNone(inst_method, "methods.MyClass.instance_method not found")
        self.assertEqual(inst_method.type, "FUNCTION")
        
        inst_usage_files = [Path(u.filePath).name for u in inst_method.usages]
        self.assertIn("caller.py", inst_usage_files)

        # Verify static_method
        stat_method = next((u for u in usages.codeUnits if u.fullyQualifiedName == "methods.MyClass.static_method"), None)
        self.assertIsNotNone(stat_method, "methods.MyClass.static_method not found")
        
        stat_usage_files = [Path(u.filePath).name for u in stat_method.usages]
        self.assertIn("caller.py", stat_usage_files)

if __name__ == '__main__':
    unittest.main()
