require_relative "findable"

module Billing
  class User
    extend Findable

    def self.build
      User.new
    end
  end

  class LegacyUser
    include Findable
  end
end
