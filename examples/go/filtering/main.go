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
	fmt.Println("Messages in time range [0, 10]:")
	count := 0
	for {
		msg, err := stream.Next()
		if err != nil {
			log.Fatal(err)
		}
		if msg == nil {
			break
		}
		if msg.LogTime > 10 {
			continue
		}
		fmt.Printf("  time=%d channel=%d\n", msg.LogTime, msg.ChannelID)
		count++
		if count >= 5 {
			break
		}
	}
	reader, err = stream.IntoReader()
	if err != nil {
		log.Fatal(err)
	}

	stream, err = reader.Messages()
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("\nMessages on channels [1]:")
	count = 0
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
		fmt.Printf("  time=%d channel=%d\n", msg.LogTime, msg.ChannelID)
		count++
		if count >= 5 {
			break
		}
	}
	reader, err = stream.IntoReader()
	if err != nil {
		log.Fatal(err)
	}

	stream, err = reader.Messages()
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("\nMessages on channel 1 in time range [0, 10]:")
	count = 0
	for {
		msg, err := stream.Next()
		if err != nil {
			log.Fatal(err)
		}
		if msg == nil {
			break
		}
		if msg.ChannelID != 1 || msg.LogTime > 10 {
			continue
		}
		fmt.Printf("  time=%d channel=%d\n", msg.LogTime, msg.ChannelID)
		count++
		if count >= 5 {
			break
		}
	}
	stream.Close()
}
