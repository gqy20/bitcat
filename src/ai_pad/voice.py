"""
语音录制与识别
使用 sounddevice 录音 + Google Speech Recognition 识别
"""

import io
import wave

import sounddevice as sd
import speech_recognition as sr


def record_and_transcribe(duration: int = 5, language: str = "zh-CN") -> str | None:
    """
    录音并转文字
    duration: 录音秒数
    language: 语言代码
    返回识别文本，失败返回 None
    """
    fs = 16000

    print("  ● 录音中...", end="", flush=True)
    try:
        audio = sd.rec(int(duration * fs), samplerate=fs, channels=1, dtype="int16")
        sd.wait()
        print(f" ({duration}s) 识别中...", end="", flush=True)
    except Exception as e:
        print(f" 录音失败: {e}")
        return None

    # 转 wav bytes
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(fs)
        wf.writeframes(audio.tobytes())

    # Google Speech Recognition
    recognizer = sr.Recognizer()
    audio_data = sr.AudioData(buf.getvalue(), fs, 2)

    try:
        text = recognizer.recognize_google(audio_data, language=language)
        print(f" 完成")
        return text
    except sr.UnknownValueError:
        print(" 未能识别")
        return None
    except sr.RequestError as e:
        print(f" 识别服务错误: {e}")
        return None
