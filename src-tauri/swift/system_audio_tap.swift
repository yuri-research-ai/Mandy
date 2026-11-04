// System-audio capture for meeting recording, via Core Audio process taps
// (macOS 14.2+). Captures everything the default output plays, except this
// process, so Handy's own feedback sounds are not recorded.
//
// Requires the "System Audio Recording" TCC permission, prompted through
// NSAudioCaptureUsageDescription in Info.plist.

import CoreAudio
import Foundation

private let unknownObject = AudioObjectID(kAudioObjectUnknown)

/// Largest number of frames handed to Rust in one callback. Core Audio can
/// deliver larger buffers, so they are split into chunks of this size.
private let maxFramesPerCallback = 4096
private let maxChannels = 8

private func setError(_ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?, _ message: String) {
    out?.pointee = strdup(message)
}

private func checkStatus(_ status: OSStatus, _ what: String) throws {
    if status != noErr {
        throw TapError.coreAudio(what, status)
    }
}

private enum TapError: Error, CustomStringConvertible {
    case unsupportedOS
    case coreAudio(String, OSStatus)
    case noOutputDevice
    case noSelfProcessObject

    var description: String {
        switch self {
        case .unsupportedOS:
            return "System audio capture requires macOS 14.2 or later."
        case let .coreAudio(what, status):
            return "\(what) failed (OSStatus \(status))."
        case .noOutputDevice:
            return "No default output device is available."
        case .noSelfProcessObject:
            return "Could not find Handy's audio process object."
        }
    }
}

@available(macOS 14.2, *)
private final class SystemTap {
    let callback: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<Float>?, Int, UInt32, Double) -> Void
    let user: UnsafeMutableRawPointer?

    private var tapID = unknownObject
    private var aggregateID = unknownObject
    private var procID: AudioDeviceIOProcID?
    private var sampleRate: Double = 0
    private var channels: UInt32 = 0
    private var nonInterleaved = false
    private let scratch: UnsafeMutablePointer<Float>

    init(callback: @escaping @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<Float>?, Int, UInt32, Double) -> Void,
         user: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.user = user
        self.scratch = UnsafeMutablePointer<Float>.allocate(capacity: maxFramesPerCallback * maxChannels)
    }

    deinit {
        scratch.deallocate()
    }

    func start() throws {
        let selfProcess = try Self.processObjectForCurrentPID()
        let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [selfProcess])
        description.uuid = UUID()
        description.name = "Handy system audio"
        description.isPrivate = true
        // muteBehavior defaults to unmuted: the user still hears the audio.

        var newTap = unknownObject
        try checkStatus(AudioHardwareCreateProcessTap(description, &newTap), "AudioHardwareCreateProcessTap")
        tapID = newTap

        let format = try Self.tapFormat(tapID)
        sampleRate = format.mSampleRate
        channels = format.mChannelsPerFrame
        nonInterleaved = (format.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0
        guard channels > 0, channels <= UInt32(maxChannels) else {
            throw TapError.coreAudio("Tap channel count \(channels) is not supported", noErr)
        }

        let outputUID = try Self.defaultOutputUID()
        let tapUID = description.uuid.uuidString
        let aggregateDescription: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Handy System Audio",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: outputUID,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [
                [kAudioSubDeviceUIDKey: outputUID],
            ],
            kAudioAggregateDeviceTapListKey: [
                [
                    kAudioSubTapDriftCompensationKey: true,
                    kAudioSubTapUIDKey: tapUID,
                ],
            ],
        ]
        var newAggregate = unknownObject
        try checkStatus(
            AudioHardwareCreateAggregateDevice(aggregateDescription as CFDictionary, &newAggregate),
            "AudioHardwareCreateAggregateDevice"
        )
        aggregateID = newAggregate

        var newProc: AudioDeviceIOProcID?
        try checkStatus(
            AudioDeviceCreateIOProcIDWithBlock(&newProc, aggregateID, nil) { [weak self] _, inputData, _, _, _ in
                self?.handleInput(inputData)
            },
            "AudioDeviceCreateIOProcIDWithBlock"
        )
        procID = newProc

        try checkStatus(AudioDeviceStart(aggregateID, procID), "AudioDeviceStart")
    }

    func stop() {
        if let procID, aggregateID != unknownObject {
            // Stop is synchronous: no IO callback runs after it returns.
            AudioDeviceStop(aggregateID, procID)
            AudioDeviceDestroyIOProcID(aggregateID, procID)
            self.procID = nil
        }
        if aggregateID != unknownObject {
            AudioHardwareDestroyAggregateDevice(aggregateID)
            aggregateID = unknownObject
        }
        if tapID != unknownObject {
            AudioHardwareDestroyProcessTap(tapID)
            tapID = unknownObject
        }
    }

    /// Runs on Core Audio's IO thread. Converts to interleaved f32 in the
    /// preallocated scratch buffer and forwards it to Rust without allocating.
    private func handleInput(_ inputData: UnsafePointer<AudioBufferList>?) {
        guard let inputData else { return }
        let list = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: inputData))
        let ch = Int(channels)

        if nonInterleaved {
            // One buffer per channel, each holding mono samples.
            guard list.count >= ch else { return }
            let frames = Int(list[0].mDataByteSize) / MemoryLayout<Float>.size
            var start = 0
            while start < frames {
                let count = min(maxFramesPerCallback, frames - start)
                for c in 0 ..< ch {
                    guard let data = list[c].mData?.assumingMemoryBound(to: Float.self) else { continue }
                    for f in 0 ..< count {
                        scratch[f * ch + c] = data[start + f]
                    }
                }
                deliver(count)
                start += count
            }
        } else {
            // A single buffer that is already interleaved.
            guard list.count >= 1, let raw = list[0].mData?.assumingMemoryBound(to: Float.self) else { return }
            let totalFrames = Int(list[0].mDataByteSize) / (MemoryLayout<Float>.size * ch)
            var start = 0
            while start < totalFrames {
                let count = min(maxFramesPerCallback, totalFrames - start)
                let src = raw + start * ch
                scratch.update(from: src, count: count * ch)
                deliver(count)
                start += count
            }
        }
    }

    private func deliver(_ frames: Int) {
        callback(user, UnsafePointer(scratch), frames, channels, sampleRate)
    }

    // MARK: Core Audio queries

    private static func processObjectForCurrentPID() throws -> AudioObjectID {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var pid = getpid()
        var object = unknownObject
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address,
            UInt32(MemoryLayout<pid_t>.size),
            &pid,
            &size,
            &object
        )
        try checkStatus(status, "Looking up Handy's audio process")
        guard object != unknownObject else { throw TapError.noSelfProcessObject }
        return object
    }

    private static func tapFormat(_ tap: AudioObjectID) throws -> AudioStreamBasicDescription {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioTapPropertyFormat,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var format = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        try checkStatus(AudioObjectGetPropertyData(tap, &address, 0, nil, &size, &format), "Reading tap format")
        return format
    }

    private static func defaultOutputUID() throws -> String {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var device = unknownObject
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        try checkStatus(
            AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &device),
            "Reading default output device"
        )
        guard device != unknownObject else { throw TapError.noOutputDevice }

        var uidAddress = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceUID,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var uid: CFString = "" as CFString
        var uidSize = UInt32(MemoryLayout<CFString>.size)
        try checkStatus(AudioObjectGetPropertyData(device, &uidAddress, 0, nil, &uidSize, &uid), "Reading output device UID")
        return uid as String
    }
}

// MARK: - C ABI

@_cdecl("handy_tap_supported")
public func handy_tap_supported() -> Bool {
    if #available(macOS 14.2, *) {
        return true
    }
    return false
}

@_cdecl("handy_tap_start")
public func handy_tap_start(
    _ callback: @escaping @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<Float>?, Int, UInt32, Double) -> Void,
    _ user: UnsafeMutableRawPointer?,
    _ outError: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 14.2, *) else {
        setError(outError, TapError.unsupportedOS.description)
        return nil
    }
    let tap = SystemTap(callback: callback, user: user)
    do {
        try tap.start()
    } catch {
        tap.stop()
        setError(outError, String(describing: error))
        return nil
    }
    return Unmanaged.passRetained(tap).toOpaque()
}

@_cdecl("handy_tap_stop")
public func handy_tap_stop(_ handle: UnsafeMutableRawPointer?) {
    guard #available(macOS 14.2, *), let handle else { return }
    let tap = Unmanaged<SystemTap>.fromOpaque(handle).takeRetainedValue()
    tap.stop()
}

@_cdecl("handy_tap_free_error")
public func handy_tap_free_error(_ error: UnsafeMutablePointer<CChar>?) {
    free(error)
}
