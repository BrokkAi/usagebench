package worker

type Worker struct{}

func (Worker) Record() {}

type Recorder interface {
	Record()
}

func NewWorker() Worker { return Worker{} }
