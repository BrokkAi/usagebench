import unittest
import tempfile
import shutil
from pathlib import Path
import sys

# Ensure the parent directory is in sys.path
parent_dir = str(Path(__file__).parent.parent)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from analyzer import analyze

class TestVariableUsages(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.root = Path(self.test_dir)
        
    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_variable_and_field_usages(self):
        # Create vars.py
        (self.root / "vars.py").write_text("""
GLOBAL_CONST = "config"

class Data:
    class_var = 42
    def __init__(self):
        self.instance_var = []
""")
        
        # Create user.py
        (self.root / "user.py").write_text("""
from vars import GLOBAL_CONST, Data

def main():
    print(GLOBAL_CONST)
    d = Data()
    print(Data.class_var)
    d.instance_var.append(1)
""")

        usages = analyze(self.root)
        
        def find_unit(fqn, unit_type):
            return next((u for u in usages.codeUnits if u.fullyQualifiedName == fqn and u.type == unit_type), None)

        # 1. Check GLOBAL_CONST
        global_const = find_unit("vars.GLOBAL_CONST", "VARIABLE")
        self.assertIsNotNone(global_const, "Could not find vars.GLOBAL_CONST as VARIABLE")
        self.assertTrue(any("user.py" in u.filePath for u in global_const.usages), "Usage of GLOBAL_CONST not found in user.py")

        # 2. Check class_var
        class_var = find_unit("vars.Data.class_var", "FIELD")
        self.assertIsNotNone(class_var, "Could not find vars.Data.class_var as FIELD")
        self.assertTrue(any("user.py" in u.filePath for u in class_var.usages), "Usage of class_var not found in user.py")

        # 3. Check instance_var
        # Jedi usually identifies self.instance_var in __init__ as vars.Data.instance_var or vars.Data.__init__.instance_var
        # We search by suffix to be flexible with Jedi's FQN resolution for instance attributes
        instance_var = next((u for u in usages.codeUnits if u.fullyQualifiedName.endswith("instance_var") and u.type == "FIELD"), None)
        self.assertIsNotNone(instance_var, "Could not find instance_var as FIELD")
        self.assertTrue(any("user.py" in u.filePath for u in instance_var.usages), "Usage of instance_var not found in user.py")

if __name__ == '__main__':
    unittest.main()
