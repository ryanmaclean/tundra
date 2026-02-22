// Animation capture helper
// Provides utilities for recording and testing sidebar animations

class AnimationCaptureHelper {
    constructor() {
        this.isRecording = false;
        this.frames = [];
        this.startTime = null;
        this.mediaRecorder = null;
    }

    // Test the sidebar animation with visual feedback
    async testAndCapture() {
        console.log('🎥 Starting animation test and capture...');
        
        // Run tests first
        await window.sidebarTester.runAllTests();
        
        // Then capture animation sequence
        await this.captureAnimationSequence();
    }

    // Capture a complete animation sequence (collapse → expand)
    async captureAnimationSequence() {
        console.log('📹 Recording animation sequence...');
        
        const sidebar = document.querySelector('.sidebar');
        const toggle = document.querySelector('.sidebar-toggle-btn');
        
        if (!sidebar || !toggle) {
            console.error('❌ Sidebar or toggle button not found');
            return;
        }

        // Start recording if possible
        try {
            await this.startRecording();
        } catch (error) {
            console.warn('⚠️ Could not start recording:', error.message);
            console.log('💡 You can manually record using Cmd+Shift+5 (macOS)');
        }

        // Animation sequence
        console.log('▶️  Starting animation sequence...');
        
        // 1. Show initial state (expanded)
        await this.wait(1000);
        console.log('📸 Initial state: Expanded');
        
        // 2. Collapse
        console.log('🔄 Collapsing sidebar...');
        toggle.click();
        await this.wait(500); // Wait for transition
        console.log('✅ Sidebar collapsed');
        
        // 3. Hold collapsed state
        await this.wait(1000);
        console.log('⏸️ Holding collapsed state');
        
        // 4. Expand
        console.log('🔄 Expanding sidebar...');
        toggle.click();
        await this.wait(500); // Wait for transition
        console.log('✅ Sidebar expanded');
        
        // 5. Hold expanded state
        await this.wait(1000);
        console.log('⏸️ Holding expanded state');
        
        // Stop recording
        try {
            await this.stopRecording();
        } catch (error) {
            console.warn('⚠️ Could not stop recording:', error.message);
        }
        
        console.log('🎬 Animation sequence complete!');
        this.generateReport();
    }

    // Start screen recording (if supported)
    async startRecording() {
        if (!navigator.mediaDevices || !navigator.mediaDevices.getDisplayMedia) {
            throw new Error('Screen recording not supported');
        }

        const stream = await navigator.mediaDevices.getDisplayMedia({
            video: {
                mediaSource: 'screen',
                width: { ideal: 1920 },
                height: { ideal: 1080 }
            }
        });

        this.mediaRecorder = new MediaRecorder(stream);
        this.frames = [];
        this.startTime = Date.now();
        this.isRecording = true;

        this.mediaRecorder.ondataavailable = (event) => {
            if (event.data.size > 0) {
                this.frames.push(event.data);
            }
        };

        this.mediaRecorder.onstop = () => {
            this.isRecording = false;
            this.processRecording();
        };

        this.mediaRecorder.start();
        console.log('🔴 Recording started...');
    }

    // Stop recording
    async stopRecording() {
        if (!this.mediaRecorder || !this.isRecording) {
            throw new Error('No active recording');
        }

        this.mediaRecorder.stop();
        console.log('⏹️ Recording stopped');
    }

    // Process recorded frames
    async processRecording() {
        if (this.frames.length === 0) {
            console.log('📹 No frames recorded');
            return;
        }

        console.log(`📹 Processing ${this.frames.length} frames...`);
        
        // Create blob from frames
        const blob = new Blob(this.frames, { type: 'video/webm' });
        
        // Create download link
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `sidebar-animation-${Date.now()}.webm`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
        
        console.log('💾 Video saved to downloads');
    }

    // Generate performance report
    generateReport() {
        const sidebar = document.querySelector('.sidebar');
        const computedStyle = window.getComputedStyle(sidebar);
        
        const report = {
            timestamp: new Date().toISOString(),
            browser: navigator.userAgent,
            sidebar: {
                width: computedStyle.width,
                minWidth: computedStyle.minWidth,
                transitionDuration: computedStyle.transitionDuration,
                transitionTiming: computedStyle.transitionTimingFunction,
                transitionProperty: computedStyle.transitionProperty
            },
            performance: {
                width: window.innerWidth,
                height: window.innerHeight,
                pixelRatio: window.devicePixelRatio
            }
        };

        console.log('📊 Animation Performance Report:');
        console.log('=====================================');
        console.log(JSON.stringify(report, null, 2));
        console.log('=====================================');

        // Save report to file
        const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `sidebar-report-${Date.now()}.json`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
        
        console.log('📄 Performance report saved to downloads');
    }

    // Manual capture helper
    async manualCapture() {
        console.log('📸 Manual capture mode');
        console.log('💡 Instructions:');
        console.log('1. Press Cmd+Shift+5 to start screen recording');
        console.log('2. Click the sidebar toggle button a few times');
        console.log('3. Press Cmd+Shift+5 again to stop recording');
        console.log('4. The video will be saved automatically');
        
        // Visual indicator
        const indicator = document.createElement('div');
        indicator.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            background: #ff6b6b;
            color: white;
            padding: 10px 20px;
            border-radius: 8px;
            font-family: monospace;
            font-size: 14px;
            z-index: 10000;
            box-shadow: 0 4px 12px rgba(0,0,0,0.3);
        `;
        indicator.textContent = '🔴 RECORDING - Press Cmd+Shift+5 to stop';
        document.body.appendChild(indicator);

        // Auto-remove after 10 seconds
        setTimeout(() => {
            if (document.body.contains(indicator)) {
                document.body.removeChild(indicator);
            }
        }, 10000);
    }

    // Helper function to wait
    wait(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    // Performance monitoring
    measurePerformance() {
        const sidebar = document.querySelector('.sidebar');
        const toggle = document.querySelector('.sidebar-toggle-btn');
        
        if (!sidebar || !toggle) {
            console.error('❌ Elements not found for performance measurement');
            return;
        }

        console.log('📏 Measuring animation performance...');
        
        // Measure collapse performance
        const startTime = performance.now();
        toggle.click();
        
        // Use PerformanceObserver to measure animation
        const observer = new PerformanceObserver((list) => {
            const entries = list.getEntries();
            entries.forEach(entry => {
                if (entry.name.includes('sidebar') || entry.name.includes('width')) {
                    console.log(`📊 Performance entry:`, entry);
                }
            });
        });

        observer.observe({ entryTypes: ['measure', 'paint', 'layout'] });
        
        // Stop observing after animation
        setTimeout(() => {
            observer.disconnect();
            const endTime = performance.now();
            console.log(`⏱️ Animation duration: ${endTime - startTime}ms`);
        }, 600);
    }
}

// Make available globally
window.animationHelper = new AnimationCaptureHelper();

// Auto-setup instructions
console.log('🎥 Animation Capture Helper loaded!');
console.log('Commands:');
console.log('  animationHelper.testAndCapture() - Run tests and record animation');
console.log('  animationHelper.captureAnimationSequence() - Record animation only');
console.log('  animationHelper.manualCapture() - Manual recording mode');
console.log('  animationHelper.measurePerformance() - Measure animation performance');
