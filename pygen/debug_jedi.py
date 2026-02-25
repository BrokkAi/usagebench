import jedi
from pathlib import Path
import tempfile
import shutil

def main():
    print(f"Jedi version: {jedi.__version__}")
    
    # Create a temporary directory for the project
    tmp_dir = tempfile.mkdtemp()
    try:
        root = Path(tmp_dir)
        code_path = root / "example.py"
        code_path.write_text("x = 10\nprint(x)")
        
        # 1. Create a simple jedi.Project
        project = jedi.Project(path=str(root))
        
        # 2. Create a jedi.Script with a variable definition
        script = jedi.Script(path=str(code_path), project=project)
        
        # 3. Call script.get_names(definitions=True)
        names = script.get_names(definitions=True)
        
        if not names:
            print("No names found.")
            return

        # Find the 'x' definition
        x_name = next((n for n in names if n.name == 'x'), names[0])
        
        print(f"\nInspecting Name object for: {x_name.full_name} ({x_name.type})")
        print("-" * 40)
        
        # 4. Print dir(name) for the found definition Name object
        attributes = dir(x_name)
        for attr in sorted(attributes):
            # Filter for methods/properties that look relevant to references or usages
            if any(term in attr.lower() for term in ["ref", "usage", "goto", "infer"]):
                print(f"FOUND: {attr}")
            else:
                # Still print all so we can see the full API
                pass
        
        print("\nAll attributes:")
        print(attributes)

    finally:
        shutil.rmtree(tmp_dir)

if __name__ == "__main__":
    main()
