class BackgroundWorker
  def perform
    OrderService.new.process
  end
end
