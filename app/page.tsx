'use client';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { AnimatePresence, motion } from 'framer-motion';

export default function HomePage() {
  const [videoSource, setVideoSource] = useState<string>('');
  const [startTime, setStartTime] = useState<string>('00:00:00');
  const [endTime, setEndTime] = useState<string>('00:00:10');
  const [ratio, setRatio] = useState<string>('Original');
  const [message, setMessage] = useState<string>('');
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [ffmpegStatus, setFfmpegStatus] =
    useState<string>('Checking FFmpeg...');
  const [isFfmpegReady, setIsFfmpegReady] = useState<boolean>(false);

  // Effect to check for FFmpeg on component mount
  useEffect(() => {
    // Listen for status updates from the Rust backend
    const unlisten = listen<string>('ffmpeg_status', (event) => {
      console.log('FFmpeg status update:', event.payload);
      setFfmpegStatus(event.payload);
      if (event.payload === 'FFmpeg is ready.') {
        setIsFfmpegReady(true);
      }
    });

    // Invoke the command to start the check/download process
    invoke('ensure_ffmpeg_is_ready').catch(console.error);

    // Cleanup listener on component unmount
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleFileSelect = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Video',
            extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm'],
          },
        ],
      });
      if (typeof selected === 'string') {
        setVideoSource(selected);
        setMessage(`Selected local file: ${selected.split(/[\\/]/).pop()}`);
      }
    } catch (error) {
      setMessage(`Error selecting file: ${error}`);
    }
  }, []);

  const handleTrimVideo = useCallback(async () => {
    if (!videoSource) {
      setMessage('Please paste a video URL or select a local file.');
      return;
    }

    setIsLoading(true);
    setMessage('Processing video...');
    try {
      const result: string = await invoke('trim_video', {
        videoSource,
        startTime,
        endTime,
        ratio,
      });
      setMessage(result);
    } catch (error: any) {
      setMessage(`Error: ${error}`);
    } finally {
      setIsLoading(false);
    }
  }, [videoSource, startTime, endTime, ratio]);

  const isUiDisabled = !isFfmpegReady || isLoading;

  return (
    <main className="flex min-h-screen flex-col items-center justify-center p-8 font-mono bg-gradient-to-br from-black via-purple-950 to-black text-white">
      <AnimatePresence>
        {!isFfmpegReady && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="absolute inset-0 bg-black bg-opacity-80 flex flex-col items-center justify-center z-50"
          >
            <p className="text-xl mb-4">{ffmpegStatus}</p>
            {ffmpegStatus.includes('Downloading') && (
              <div className="w-1/4 h-2 bg-gray-700 rounded-full overflow-hidden">
                <motion.div
                  className="h-full bg-white"
                  initial={{ width: 0 }}
                  animate={{ width: '100%' }}
                  transition={{
                    duration: 1,
                    repeat: Infinity,
                    repeatType: 'reverse',
                    ease: 'easeInOut',
                  }}
                />
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      <motion.h1
        initial={{ opacity: 0, y: -50 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5 }}
        className="text-4xl font-bold mb-8 text-center"
      >
        What do you wanna clip?
      </motion.h1>

      <motion.div
        initial={{ opacity: 0, scale: 0.9 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.5, delay: 0.2 }}
        className="w-full max-w-xl bg-black bg-opacity-70 p-8 rounded-lg shadow-lg border border-gray-800"
      >
        <fieldset disabled={isUiDisabled} className="space-y-6">
          <div>
            <Label htmlFor="video-source" className="sr-only">
              Video Source
            </Label>
            <div className="flex gap-2">
              <Input
                id="video-source"
                placeholder="Paste video url here..."
                value={videoSource}
                onChange={(e) => setVideoSource(e.target.value)}
                className="flex-grow bg-input text-foreground border-gray-700 placeholder:text-gray-500 disabled:opacity-50"
              />
              <Button
                onClick={handleFileSelect}
                variant="outline"
                className="bg-primary text-primary-foreground hover:bg-primary/90 border-gray-700 disabled:opacity-50"
              >
                Select File
              </Button>
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div>
              <Label
                htmlFor="start-at"
                className="text-muted-foreground mb-1 block"
              >
                Start At
              </Label>
              <Input
                id="start-at"
                type="text"
                value={startTime}
                onChange={(e) => setStartTime(e.target.value)}
                placeholder="00:00:00"
                className="bg-input text-foreground border-gray-700 disabled:opacity-50"
              />
            </div>
            <div>
              <Label
                htmlFor="end-at"
                className="text-muted-foreground mb-1 block"
              >
                End At
              </Label>
              <Input
                id="end-at"
                type="text"
                value={endTime}
                onChange={(e) => setEndTime(e.target.value)}
                placeholder="00:00:10"
                className="bg-input text-foreground border-gray-700 disabled:opacity-50"
              />
            </div>
            <div>
              <Label
                htmlFor="ratio"
                className="text-muted-foreground mb-1 block"
              >
                Ratio
              </Label>
              <Select
                value={ratio}
                onValueChange={setRatio}
                disabled={isUiDisabled}
              >
                <SelectTrigger className="w-full bg-input text-foreground border-gray-700 disabled:opacity-50">
                  <SelectValue placeholder="Select Ratio" />
                </SelectTrigger>
                <SelectContent className="bg-black text-foreground border-gray-700">
                  <SelectItem value="Original">Original</SelectItem>
                  <SelectItem value="16:9">16:9 (Landscape)</SelectItem>
                  <SelectItem value="9:16">9:16 (Portrait)</SelectItem>
                  <SelectItem value="1:1">1:1 (Square)</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <Button
            onClick={handleTrimVideo}
            disabled={isUiDisabled}
            className="w-full bg-primary text-primary-foreground hover:bg-primary/90 py-3 text-lg disabled:opacity-50"
          >
            {isLoading ? 'Clipping...' : 'Clip Video'}
          </Button>
        </fieldset>

        {message && (
          <motion.p
            key={message} // Use key to re-trigger animation on message change
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3 }}
            className={`mt-4 text-center break-words ${
              message.startsWith('Error')
                ? 'text-destructive'
                : 'text-green-400'
            }`}
          >
            {message}
          </motion.p>
        )}
      </motion.div>
    </main>
  );
}
