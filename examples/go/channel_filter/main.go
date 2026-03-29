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
	stream, err := reader.Messages()
	if err != nil {
		log.Fatal(err)
	}
	count := 0
	for {
		msg, err := stream.Next()
		if err != nil {
			log.Fatal(err)
		}
		if msg == nil {
			break
		}
		if msg.ChannelID != 1 {
			continue
		}
		fmt.Printf("example msg: channel=%d time=%d\n", msg.ChannelID, msg.LogTime)
		count++
		if count >= 5 {
			break
		}
	}
	fmt.Printf("example messages: %d\n", count)
	stream.Close()
}
