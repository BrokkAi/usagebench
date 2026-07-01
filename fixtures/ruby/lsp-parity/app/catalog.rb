require_relative "../lib/shop/product"
require_relative "../lib/shop/pricing"

product = Shop::Product.featured
sku_product = Shop::Product.from_sku("sku-1")

product.name
product.label
product.summary

Shop::Pricing.tax_rate("EU")
Shop::Discount.default
