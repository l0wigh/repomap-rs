class AnalyticsTracker(private val worker: BackgroundWorker) {
    fun track() {
        worker.perform()
    }
}
