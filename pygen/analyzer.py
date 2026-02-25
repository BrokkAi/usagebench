import jedi
import logging
from pathlib import Path
from typing import List, Set, Dict, Optional
from models import ProgramUsages, CodeUnitUsages, UsageLocation

logger = logging.getLogger(__name__)

def analyze(root_path: Path) -> ProgramUsages:
    project = jedi.Project(path=str(root_path))
    code_units: List[CodeUnitUsages] = []
    
    # Cache scripts to avoid re-parsing for every context lookup
    scripts: Dict[str, jedi.Script] = {}

    def get_script(p: str) -> jedi.Script:
        if p not in scripts:
            scripts[p] = jedi.Script(path=p, project=project)
        return scripts[p]

    def capture_snippet(lines: List[str], line_no: int) -> str:
        if not lines or line_no <= 0:
            return ""
        idx = line_no - 1
        start = max(0, idx - 3)
        end = min(len(lines), idx + 4)
        return "\n".join(lines[start:end])

    def find_enclosing_context(file_path: str, line: int, col: int) -> str:
        try:
            script = get_script(file_path)
            # get_context returns the definition (class/func) containing the position
            context = script.get_context(line=line, column=col)
            
            # If we are exactly on a class/function definition line, get_context might 
            # return that definition itself. We want the name.
            if context:
                return context.full_name or context.name or "unknown"
            return "unknown"
        except Exception:
            return "unknown"

    python_files = list(root_path.rglob("*.py"))
    logger.info(f"Analyzing {len(python_files)} Python files in {root_path}...")

    for file_path in python_files:
        try:
            # Skip tests for definitions (consistent with javagen)
            file_path_str = str(file_path).replace("\\", "/")
            if "/test/" in file_path_str or "/tests/" in file_path_str:
                continue

            script = get_script(str(file_path))
            # definitions=True finds classes and functions defined in this file.
            # all_scopes=True includes names defined inside classes/functions.
            names = script.get_names(all_scopes=True, definitions=True, references=False)
            
            for name in names:
                # Jedi marks methods as 'function' type.
                if name.type not in ('class', 'function'):
                    continue
                
                # Definition info
                decl_line = name.line
                decl_type = name.type.upper() # CLASS or FUNCTION
                fqn = name.full_name
                
                if not fqn:
                    continue

                # Find references project-wide. 
                # name.goto() ensures we are looking for references of the actual definition.
                try:
                    references = name.get_references()
                except Exception as e:
                    logger.warning(f"Failed to get references for {fqn}: {e}")
                    references = []

                usages: Set[UsageLocation] = set()

                for ref in references:
                    # Exclude the definition itself from the usage list
                    # Note: ref.module_path might be None for some system modules, but shouldn't be for project files
                    if ref.line == decl_line and (not ref.module_path or Path(ref.module_path) == file_path):
                        continue
                    
                    if not ref.module_path:
                        continue

                    u_path = Path(ref.module_path)
                    u_line = ref.line
                    
                    # Extract snippet
                    try:
                        lines = u_path.read_text(errors='replace').splitlines()
                        snippet = capture_snippet(lines, u_line)
                    except Exception:
                        snippet = ""

                    # Find enclosing context (who is using it?)
                    enclosing = find_enclosing_context(str(u_path), u_line, ref.column)

                    usages.add(UsageLocation(
                        fullyQualifiedName=enclosing,
                        lineNumber=u_line,
                        snippet=snippet,
                        filePath=str(u_path.resolve()),
                        syntaxStyle="text/x-python"
                    ))

                code_units.append(CodeUnitUsages(
                    fullyQualifiedName=fqn,
                    declarationLineNumber=decl_line,
                    type=decl_type,
                    usages=usages
                ))

        except Exception as e:
            logger.error(f"Error analyzing {file_path}: {e}")

    return ProgramUsages(codeUnits=code_units)
