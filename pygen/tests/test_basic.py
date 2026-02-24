import unittest
import tempfile
import shutil
from pathlib import Path
from pygen.analyzer import analyze

class TestPyGen(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.root = Path(self.test_dir)
        
    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_basic_usage(self):
        # Create a dummy python project
        (self.root / "utils.py").write_text("""
def hello():
    pass

class Greeter:
    def greet(self):
        hello()
""")
        
        (self.root / "main.py").write_text("""
from utils import hello, Greeter

def main():
    hello()
    g = Greeter()
    g.greet()
""")

        usages = analyze(self.root)
        
        # Check if we found 'hello' definition
        hello_def = next((u for u in usages.codeUnits if "hello" in u.fullyQualifiedName and u.type == "FUNCTION"), None)
        self.assertIsNotNone(hello_def)
        
        # Check usages of hello (should be main.py and utils.py)
        usage_files = [Path(u.filePath).name for u in hello_def.usages]
        self.assertIn("main.py", usage_files)
        self.assertIn("utils.py", usage_files)
        
        # Check Greeter definition
        greeter_def = next((u for u in usages.codeUnits if "Greeter" in u.fullyQualifiedName and u.type == "CLASS"), None)
        self.assertIsNotNone(greeter_def)

if __name__ == '__main__':
    unittest.main()
