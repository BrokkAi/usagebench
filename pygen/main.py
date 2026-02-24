import argparse
import sys
import logging
from pathlib import Path

# Ensure we can import from local modules (current directory)
sys.path.append(str(Path(__file__).parent))

from clone_util import process_repo
from analyzer import analyze

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("pygen")

def main():
    parser = argparse.ArgumentParser(description="PyGen: Python Usage Extractor")
    parser.add_argument("input", help="Input directory or CSV file")
    parser.add_argument("output", help="Output directory")
    args = parser.parse_args()

    input_path = Path(args.input).resolve()
    output_dir = Path(args.output).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    if input_path.is_file() and input_path.suffix == '.csv':
        process_csv(input_path, output_dir)
    elif input_path.is_dir():
        process_dir(input_path, output_dir)
    else:
        logger.error(f"Input {input_path} is neither a directory nor a CSV file.")
        sys.exit(1)

def process_csv(csv_path: Path, output_dir: Path):
    logger.info(f"Reading repositories from: {csv_path}")
    with open(csv_path, 'r') as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                repo_path = process_repo(line, output_dir)
                if repo_path:
                    process_dir(repo_path, output_dir)
            except Exception as e:
                logger.error(f"Failed to process line '{line}': {e}")

def process_dir(input_path: Path, output_dir: Path):
    project_name = input_path.name
    usages_file = output_dir / f"{project_name}-usages.json"

    if usages_file.exists():
        logger.info(f"{usages_file} already exists, skipping...")
        return

    logger.info(f"Analyzing usages for {input_path}...")
    try:
        usages = analyze(input_path)
        logger.info(f"Usage analysis complete, writing to {usages_file}...")
        
        with open(usages_file, 'w') as f:
            f.write(usages.model_dump_json(indent=3))
            
        logger.info("Usage analysis results successfully written.")
    except Exception as e:
        logger.error(f"Exception encountered while analyzing {input_path}: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    main()
