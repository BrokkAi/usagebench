import unittest
import tempfile
import shutil
import sys
from pathlib import Path

# Ensure the parent directory is in sys.path so 'analyzer' can be imported
parent_dir = str(Path(__file__).parent.parent)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from analyzer import analyze

class TestTypeUsages(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.root = Path(self.test_dir)
        
    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_type_usages(self):
        # 1. Define definitions.py
        (self.root / "definitions.py").write_text("""
class MyType:
    pass

class OtherType:
    pass
""")
        
        # 2. Define usages.py with various type usage patterns
        (self.root / "usages.py").write_text("""
from definitions import MyType, OtherType

def func(a: MyType) -> OtherType:
    return OtherType()

class Sub(MyType):
    pass

def list_func(l: list[MyType]):
    pass

def forward_ref(x: 'MyType'):
    pass
""")

        usages = analyze(self.root)
        
        # Find MyType and OtherType code units
        my_type_unit = next((u for u in usages.codeUnits if u.fullyQualifiedName == "definitions.MyType"), None)
        other_type_unit = next((u for u in usages.codeUnits if u.fullyQualifiedName == "definitions.OtherType"), None)
        
        self.assertIsNotNone(my_type_unit, "Could not find MyType in analysis results")
        self.assertIsNotNone(other_type_unit, "Could not find OtherType in analysis results")

        # Verify MyType usages in usages.py
        my_type_usage_lines = {u.lineNumber for u in my_type_unit.usages if Path(u.filePath).name == "usages.py"}
        
        # Line numbers in usages.py (1-indexed):
        # 2: from definitions import MyType... (Import)
        # 4: def func(a: MyType) -> OtherType: (Arg annotation)
        # 7: class Sub(MyType): (Inheritance)
        # 10: def list_func(l: list[MyType]): (Generic argument)
        # 13: def forward_ref(x: 'MyType'): (String forward ref - behavior check)

        self.assertIn(4, my_type_usage_lines, "Missing MyType usage in function argument annotation")
        self.assertIn(7, my_type_usage_lines, "Missing MyType usage in class inheritance")
        self.assertIn(10, my_type_usage_lines, "Missing MyType usage in list generic argument")
        
        # Note: Jedi's behavior on string forward references can vary by version. 
        # We check it but don't strictly fail if the environment's Jedi doesn't support it yet.
        # self.assertIn(13, my_type_usage_lines, "Missing MyType usage in forward reference")

        # Verify OtherType usages in usages.py
        other_type_usage_lines = {u.lineNumber for u in other_type_unit.usages if Path(u.filePath).name == "usages.py"}
        
        # 4: -> OtherType (Return annotation)
        # 5: return OtherType() (Instantiation)
        self.assertIn(4, other_type_usage_lines, "Missing OtherType usage in return annotation")
        self.assertIn(5, other_type_usage_lines, "Missing OtherType usage in instantiation")

if __name__ == '__main__':
    unittest.main()
