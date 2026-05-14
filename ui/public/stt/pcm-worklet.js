/**
 * pcm-worklet — AudioWorkletProcessor，把每个 render quantum 的 PCM 块
 * 通过 port 发给主线程累积。注册名 'pcm-worklet'。
 */
class PcmWorklet extends AudioWorkletProcessor {
  process(inputs) {
    const input = inputs[0]
    if (input && input[0] && input[0].length > 0) {
      // input[0] 是本 quantum 的 Float32Array（通常 128 samples）。复制后发送。
      this.port.postMessage(input[0].slice(0))
    }
    return true
  }
}
registerProcessor('pcm-worklet', PcmWorklet)
