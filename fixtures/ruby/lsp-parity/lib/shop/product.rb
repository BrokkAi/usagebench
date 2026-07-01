require_relative "pricing"

module Shop
  class Product
    attr_reader :name

    def initialize(name)
      @name = name
    end

    alias_method :label, :name

    def summary
      label
    end

    def self.featured
      new("featured")
    end

    class << self
      def from_sku(sku)
        new(sku)
      end
    end
  end

  autoload :Discount, "shop/discount"
end
