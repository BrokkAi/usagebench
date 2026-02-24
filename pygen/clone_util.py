import os
import subprocess
import logging
from pathlib import Path
from typing import Optional

logger = logging.getLogger(__name__)

def run_shell_command(cmd: list[str], cwd: Path) -> None:
    logger.info(f"  [RUN] {' '.join(cmd)} (in {cwd})")
    try:
        result = subprocess.run(
            cmd, 
            cwd=cwd, 
            check=True, 
            capture_output=True, 
            text=True
        )
        if result.stdout:
            logger.info(f"  [OUT] {result.stdout.strip()}")
    except subprocess.CalledProcessError as e:
        logger.error(f"  [ERROR] Command failed (exit code {e.returncode}): {' '.join(cmd)}")
        if e.stdout:
            logger.warning(f"  [STDOUT] {e.stdout.strip()}")
        if e.stderr:
            logger.error(f"  [STDERR] {e.stderr.strip()}")
        raise

def process_repo(line: str, base_dir: Path) -> Optional[Path]:
    parts = line.split(',')
    if len(parts) < 2:
        raise ValueError(f"Skipping malformed line: {line}")
    
    repo_url = parts[0].strip()
    commit_sha = parts[1].strip()

    if not repo_url or not commit_sha:
        raise ValueError("Repo URL or SHA is empty.")

    # Extract simple repo name
    repo_name = repo_url.split('/')[-1]
    if repo_name.endswith(".git"):
        repo_name = repo_name[:-4]
    
    repo_path = base_dir / repo_name

    logger.info(f"--- Processing {repo_name} ---")

    if repo_path.is_dir():
        # Case 1: Repo exists
        logger.info(f"Repository exists at {repo_path}. Fetching...")
        run_shell_command(["git", "fetch", "--all"], repo_path)
        logger.info(f"Checking out commit {commit_sha}...")
        run_shell_command(["git", "checkout", commit_sha], repo_path)
    else:
        # Case 2: Clone
        logger.info(f"Cloning {repo_url} to {repo_path}...")
        # git clone creates the directory, so we run in base_dir
        run_shell_command(["git", "clone", repo_url, str(repo_path)], base_dir)
        logger.info(f"Checking out commit {commit_sha}...")
        run_shell_command(["git", "checkout", commit_sha], repo_path)
    
    logger.info(f"Successfully processed {repo_name} at {commit_sha}.")
    return repo_path
