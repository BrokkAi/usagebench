module Shop
  module Pricing
    module_function

    def tax_rate(region)
      region == "EU" ? 0.2 : 0.1
    end
  end
end
