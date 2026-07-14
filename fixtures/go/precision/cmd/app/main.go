package main

import . "example.com/precision/worker"

func main() {
	worker := NewWorker()
	worker.Record()
	_, paired := 0, NewWorker()
	paired.Record()
	var recorder Recorder = worker
	recorder.Record()
}
