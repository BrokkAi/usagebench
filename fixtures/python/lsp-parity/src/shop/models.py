from dataclasses import dataclass


@dataclass
class User:
    name: str

    @property
    def normalized_name(self) -> str:
        return self.name.lower()

    @classmethod
    def guest(cls) -> "User":
        return cls("guest")

    @staticmethod
    def format_name(name: str) -> str:
        return name.title()


class DynamicConfig:
    def __getattr__(self, key: str) -> str:
        return key
