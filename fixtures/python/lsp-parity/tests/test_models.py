from shop import Account
from shop.models import DynamicConfig

user = Account.guest()
Account.format_name("ada")
user.normalized_name

config = DynamicConfig()
config.theme
