class Base:
    def save(self) -> None:
        pass


class Child(Base):
    pass


class Grandchild(Child):
    pass
