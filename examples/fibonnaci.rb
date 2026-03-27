def fibonacci(n)
  return [] if n == 0
  return [0] if n == 1
  fib = [0, 1]
  (2...n).each do |i|
    fib << fib[i-1] + fib[i-2]
  end
  fib
end

puts fibonacci(10).inspect