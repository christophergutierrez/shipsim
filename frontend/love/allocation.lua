local allocation = {}

function allocation.increment(value, maximum)
  return math.min(maximum or 0, (value or 0) + 1)
end

function allocation.decrement(value)
  return math.max(0, (value or 0) - 1)
end

return allocation
