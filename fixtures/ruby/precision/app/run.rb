require_relative "../lib/precision/factory"

service = Precision.build
service.execute

second = Precision::Base.build
second.execute
