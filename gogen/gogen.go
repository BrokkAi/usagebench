package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"gogen/internal/analyzer"
	"gogen/internal/cloner"
	"log"
	"os"
	"path/filepath"
	"strings"
)

func main() {
	input := flag.String("input", "", "Input directory of a Go project or CSV file of Git repositories ('repoUrl,commitSha')")
	output := flag.String("output", "./gogen_output", "Output directory")
	flag.Parse()

	if *input == "" {
		log.Fatal("--input is required")
	}

	absOutput, err := filepath.Abs(*output)
	if err != nil {
		log.Fatalf("Failed to resolve output path: %v", err)
	}

	if err := os.MkdirAll(absOutput, 0755); err != nil {
		log.Fatalf("Failed to create output directory: %v", err)
	}

	if isCSV(*input) {
		runCsv(*input, absOutput)
	} else {
		runDir(*input, absOutput)
	}
}

func isCSV(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir() && strings.HasSuffix(strings.ToLower(path), ".csv")
}

func runCsv(csvPath string, outputDir string) {
	log.Printf("Reading repositories from: %s", csvPath)
	file, err := os.Open(csvPath)
	if err != nil {
		log.Fatalf("Failed to open CSV file: %v", err)
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}

		parts := strings.Split(line, ",")
		if len(parts) < 2 {
			log.Printf("Skipping malformed line: %s", line)
			continue
		}

		repoUrl := parts[0]
		commitSha := parts[1]

		path, err := cloner.ProcessRepo(repoUrl, commitSha, outputDir)
		if err != nil {
			log.Printf("Failed to process repo %s: %v", repoUrl, err)
			continue
		}

		runDir(path, outputDir)
	}

	if err := scanner.Err(); err != nil {
		log.Printf("Error reading CSV: %v", err)
	}
}

func runDir(inputPath string, outputDir string) {
	absInput, err := filepath.Abs(inputPath)
	if err != nil {
		log.Printf("Failed to resolve input path %s: %v", inputPath, err)
		return
	}

	projectName := filepath.Base(absInput)
	usagesFile := filepath.Join(outputDir, fmt.Sprintf("%s-usages.json", projectName))

	if _, err := os.Stat(usagesFile); err == nil {
		log.Printf("%s already exists, skipping...", usagesFile)
		return
	}

	log.Printf("Analyzing usages in %s...", absInput)
	usages, err := analyzer.Analyze(absInput)
	if err != nil {
		log.Printf("Exception encountered while analyzing %s: %v", absInput, err)
		return
	}

	log.Printf("Usage analysis complete, writing to %s...", usagesFile)
	data, err := json.MarshalIndent(usages, "", "   ")
	if err != nil {
		log.Printf("Failed to serialize usages: %v", err)
		return
	}

	if err := os.WriteFile(usagesFile, data, 0644); err != nil {
		log.Printf("Failed to write output file: %v", err)
		return
	}
	log.Printf("Usage analysis results successfully written.")
}
