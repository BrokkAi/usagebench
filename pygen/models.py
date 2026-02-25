from typing import List, Set, Optional
from pydantic import BaseModel

class UsageLocation(BaseModel):
    fullyQualifiedName: str
    lineNumber: int
    snippet: str
    filePath: str
    syntaxStyle: str = "text/x-python"

    # Allow usage in sets (deduplication)
    def __hash__(self):
        return hash((self.fullyQualifiedName, self.lineNumber, self.filePath))

    def __eq__(self, other):
        if not isinstance(other, UsageLocation):
            return False
        return (self.fullyQualifiedName, self.lineNumber, self.filePath) == (other.fullyQualifiedName, other.lineNumber, other.filePath)

class CodeUnitUsages(BaseModel):
    fullyQualifiedName: str
    declarationLineNumber: int
    type: str
    usages: Set[UsageLocation]

class ProgramUsages(BaseModel):
    codeUnits: List[CodeUnitUsages]
