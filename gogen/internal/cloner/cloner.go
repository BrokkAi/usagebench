package cloner

import (
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// ProcessRepo clones a repository or fetches updates, then checks out a specific commit.
// Returns the absolute path to the repo and any error.
func ProcessRepo(repoUrl, commitSha, baseDir string) (string, error) {
	repoUrl = strings.TrimSpace(repoUrl)
	commitSha = strings.TrimSpace(commitSha)

	if repoUrl == "" || commitSha == "" {
		return "", fmt.Errorf("repo URL or SHA is empty")
	}

	// Extract repo name: e.g., "https://github.com/user/repo.git" -> "repo"
	parts := strings.Split(repoUrl, "/")
	lastPart := parts[len(parts)-1]
	repoName := strings.TrimSuffix(lastPart, ".git")
	
	repoPath := filepath.Join(baseDir, repoName)
	absPath, err := filepath.Abs(repoPath)
	if err != nil {
		return "", err
	}

	log.Printf("--- Processing %s ---", repoName)

	if info, err := os.Stat(repoPath); err == nil && info.IsDir() {
		log.Printf("Repository exists at %s. Fetching...", repoPath)
		if err := runGit(repoPath, "fetch", "--all"); err != nil {
			return "", err
		}
	} else {
		log.Printf("Cloning %s to %s...", repoUrl, repoPath)
		// Ensure baseDir exists
		if err := os.MkdirAll(baseDir, 0755); err != nil {
			return "", err
		}
		if err := runGit(baseDir, "clone", repoUrl, repoPath); err != nil {
			return "", err
		}
	}

	log.Printf("Checking out commit %s...", commitSha)
	if err := runGit(repoPath, "checkout", commitSha); err != nil {
		return "", err
	}

	log.Printf("Successfully processed %s at %s.", repoName, commitSha)
	return absPath, nil
}

func runGit(dir string, args ...string) error {
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	log.Printf("  [RUN] git %s (in %s)", strings.Join(args, " "), dir)
	
	output, err := cmd.CombinedOutput()
	if err != nil {
		log.Printf("  [ERROR] Command failed: %v", err)
		if len(output) > 0 {
			log.Printf("  [STDOUT/ERR] %s", string(output))
		}
		return fmt.Errorf("git command failed: %w", err)
	}
	
	if len(output) > 0 {
		log.Printf("  [OUT] %s", string(output))
	}
	return nil
}
