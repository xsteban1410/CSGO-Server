package main

import (
	"context"
	"fmt"
	"log"
	"os/exec"
	"strings"
	"time"

	"github.com/xsteban1410/CSGO-Server/go/internal/sched"
)

const (
	taskTimeout = 30 * time.Minute
	historyKeep = 200
)

// taskRun is a finished task, for the Scheduler tab.
type taskRun struct {
	ID     string `json:"id"`
	Start  int64  `json:"start"`
	End    int64  `json:"end"`
	OK     bool   `json:"ok"`
	Output string `json:"output,omitempty"`
}

func (a *agent) taskList() ([]sched.Task, error) { return sched.Load(a.sched) }

func (a *agent) setTasks(tasks []sched.Task) error { return sched.Save(a.sched, tasks) }

func (a *agent) taskHistory() []taskRun {
	a.mu.Lock()
	defer a.mu.Unlock()
	return append([]taskRun{}, a.history...)
}

func (a *agent) runNow(id string) error {
	tasks, err := sched.Load(a.sched)
	if err != nil {
		return err
	}
	for _, t := range tasks {
		if t.ID == id {
			return a.launch(t)
		}
	}
	return fmt.Errorf("no task %q", id)
}

// launch starts a task unless another one is running: they act on the same server, so they never overlap.
func (a *agent) launch(t sched.Task) error {
	cmds, err := t.Commands()
	if err != nil {
		return err
	}
	a.mu.Lock()
	if len(a.running) > 0 {
		a.mu.Unlock()
		return fmt.Errorf("another task is running, %q was not started", t.ID)
	}
	a.running[t.ID] = true
	a.mu.Unlock()
	go func() {
		start := time.Now()
		run := taskRun{ID: t.ID, Start: start.Unix(), OK: true}
		ctx, cancel := context.WithTimeout(context.Background(), taskTimeout)
		defer cancel()
		for _, args := range cmds {
			// no shell: the arguments go as they are, and Task.Commands only allows the listed actions
			out, err := exec.CommandContext(ctx, a.cli, args...).CombinedOutput()
			run.Output += redact(string(out), a.cfg.Secrets)
			if err != nil {
				run.OK = false
				run.Output += err.Error()
				break
			}
		}
		run.End = time.Now().Unix()
		run.Output = strings.TrimSpace(run.Output)
		if len(run.Output) > 4096 {
			run.Output = run.Output[len(run.Output)-4096:]
		}
		a.mu.Lock()
		delete(a.running, t.ID)
		a.history = append(a.history, run)
		if extra := len(a.history) - historyKeep; extra > 0 {
			a.history = a.history[extra:]
		}
		a.mu.Unlock()
		if a.db != nil {
			state := "failed"
			if run.OK {
				state = "done"
			}
			if err := a.db.Event(time.Now(), "task", t.ID+" "+state); err != nil {
				log.Printf("stats: %v", err)
			}
		}
	}()
	return nil
}

// scheduler looks once a minute at which tasks are due. It reloads the file, so an edit applies without a restart.
func (a *agent) scheduler(ctx context.Context) {
	t := time.NewTicker(30 * time.Second)
	defer t.Stop()
	due := map[string]time.Time{}
	for {
		now := time.Now()
		tasks, err := sched.Load(a.sched)
		if err != nil {
			log.Printf("schedule: %v", err)
		}
		active := map[string]bool{}
		for _, task := range tasks {
			if !task.Enabled {
				continue
			}
			active[task.ID] = true
			next, ok := due[task.ID]
			if !ok {
				due[task.ID] = task.Next(now)
				continue
			}
			if !now.Before(next) {
				due[task.ID] = task.Next(now)
				if err := a.launch(task); err != nil {
					log.Print(err)
				}
			}
		}
		for id := range due {
			if !active[id] {
				delete(due, id)
			}
		}
		select {
		case <-ctx.Done():
			return
		case <-t.C:
		}
	}
}
