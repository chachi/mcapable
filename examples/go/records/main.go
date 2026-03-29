package main

import (
	"fmt"
	"log"

	"github.com/mcapable/mcapable-go"
	"mcapable-go-examples/common"
)

func main() {
	reader, err := mcapable.NewReaderFromBytes(common.SampleBytes())
	if err != nil {
		log.Fatal(err)
	}

	stream, err := reader.Records()
	if err != nil {
		log.Fatal(err)
	}
	count := 0
	for {
		record, err := stream.Next()
		if err != nil {
			log.Fatal(err)
		}
		if record == nil {
			break
		}
		count++
		if count <= 5 {
			fmt.Printf("Record: %s\n", record.Kind)
		}
	}
	fmt.Printf("Total records: %d\n", count)
	stream.Close()
}
