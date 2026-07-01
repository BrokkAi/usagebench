package main

import svc "example.com/parity/pkg/service"

func main() {
	worker := svc.NewWorker()
	worker.Record("start")
	_ = worker.Last
	var runner svc.Runner = worker
	_ = runner.Run()
}
