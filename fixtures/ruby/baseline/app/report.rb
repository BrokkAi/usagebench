require_relative "../lib/billing/invoice"
require_relative "../lib/billing/user"

module Reports
  class InvoiceReport
    def render
      invoice = Billing::Invoice.build
      invoice.audit
      invoice.total_label
      Billing::Invoice::DEFAULT_CURRENCY
      Billing::User.find(42)
      Billing::LegacyUser.new.find(7)
      normalize_total(19)
      Billing::User.build
      invoice.public_send(:audit)
      Billing::Invoice.last_build
    end
  end
end

def normalize_total(value)
  value.round + SCRIPT_LIMIT
end

SCRIPT_LIMIT = 100
