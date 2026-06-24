require_relative "record"
require_relative "money"
require_relative "auditable"
require_relative "formatting"

module Billing
  class Invoice < Record
    include Auditable
    prepend Formatting

    DEFAULT_CURRENCY = Money::Currency.new("USD")
    @@sequence = 0
    @last_build = nil

    def initialize
      @status = "draft"
    end

    def status
      @status
    end

    def total_label
      "from-invoice"
    end

    def self.build
      @@sequence += 1
      @last_build = Invoice.new
    end

    def self.last_build
      @last_build
    end
  end
end
