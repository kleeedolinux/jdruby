# Define a module for logging functionality
module Logger
  def log(message)
    puts "[#{Time.now}] #{message}"
  end
end

# Base class for all tasks
class Task
  include Logger

  attr_reader :name

  def initialize(name, &block)
    @name = name
    @action = block
  end

  # Execute the task
  def run
    log("Starting task: #{@name}")
    @action.call if @action
    log("Finished task: #{@name}")
  end
end

# Scheduler to manage multiple tasks
class Scheduler
  include Logger

  def initialize
    @tasks = []
  end

  # Add a task dynamically
  def add_task(name, &block)
    task = Task.new(name, &block)
    @tasks << task
    log("Added task: #{name}")
  end

  # Run all tasks
  def run_all
    log("Running all tasks...")
    @tasks.each(&:run)
    log("All tasks completed!")
  end

  # Metaprogramming: dynamically create a shortcut to add a specific type of task
  def self.create_task_type(type_name)
    define_method("add_#{type_name}_task") do |name, &block|
      add_task("#{type_name.capitalize}: #{name}", &block)
    end
  end
end

# Dynamically create two custom task types
Scheduler.create_task_type(:email)
Scheduler.create_task_type(:backup)

# Example usage
scheduler = Scheduler.new

# Add generic task
scheduler.add_task("Clean temp files") do
  puts "Deleting temp files..."
end

# Add dynamic task types
scheduler.add_email_task("Send welcome email") do
  puts "Email sent to new users!"
end

scheduler.add_backup_task("Database backup") do
  puts "Database backed up successfully!"
end

# Run all tasks
scheduler.run_all