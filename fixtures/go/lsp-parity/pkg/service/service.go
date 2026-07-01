package service

type AuditLog struct {
	Last string
}

func (a *AuditLog) Record(message string) string {
	a.Last = message
	return a.Last
}

type Worker struct {
	*AuditLog
}

func NewWorker() *Worker {
	return &Worker{AuditLog: &AuditLog{}}
}

type Runner interface {
	Run() string
}

func (w *Worker) Run() string {
	return w.Record("run")
}
