from precision import Child
from precision import Grandchild


def run() -> None:
    client: Child = Child()
    client.save()
    grandchild: Grandchild = Grandchild()
    grandchild.save()
