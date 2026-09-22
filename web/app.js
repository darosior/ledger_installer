// Bacca Web UI - JavaScript application
// Interfaces with the WASM module for Ledger device management

let wasm = null;
let isConnected = false;

// Initialize the application
async function init() {
    log('Initializing Bacca...', 'info');

    try {
        // Load WASM module
        wasm = await import('./pkg/ledger_manager_web.js');
        await wasm.default();
        log('WASM module loaded', 'success');

        // Check WebHID support
        if (!wasm.check_webhid_support()) {
            document.getElementById('support-warning').classList.remove('hidden');
            log('WebHID not supported in this browser', 'error');
            disableAllButtons();
            return;
        }

        log('WebHID supported', 'success');
        setupEventListeners();

    } catch (error) {
        log(`Failed to initialize: ${error}`, 'error');
        console.error('Init error:', error);
    }
}

// Set up button event listeners
function setupEventListeners() {
    // Connect button
    document.getElementById('connect-btn').addEventListener('click', connectDevice);

    // Refresh info button
    document.getElementById('refresh-info-btn').addEventListener('click', refreshDeviceInfo);

    // Genuine check button
    document.getElementById('genuine-btn').addEventListener('click', runGenuineCheck);

    // Bitcoin mainnet buttons
    document.getElementById('install-bitcoin-btn').addEventListener('click', () => installBitcoinApp(false));
    document.getElementById('update-bitcoin-btn').addEventListener('click', () => updateBitcoinApp(false));
    document.getElementById('open-bitcoin-btn').addEventListener('click', () => openBitcoinApp(false));

    // Bitcoin testnet buttons
    document.getElementById('install-bitcoin-test-btn').addEventListener('click', () => installBitcoinApp(true));
    document.getElementById('update-bitcoin-test-btn').addEventListener('click', () => updateBitcoinApp(true));
    document.getElementById('open-bitcoin-test-btn').addEventListener('click', () => openBitcoinApp(true));

    // Clear log button
    document.getElementById('clear-log-btn').addEventListener('click', clearLog);
}

// Connect to Ledger device
async function connectDevice() {
    const btn = document.getElementById('connect-btn');
    const originalText = btn.textContent;
    btn.disabled = true;
    btn.innerHTML = '<span class="loading"></span>Connecting...';

    try {
        log('Requesting device connection...', 'info');
        const result = await wasm.connect_device();

        if (result.success) {
            isConnected = true;
            updateConnectionStatus(true);
            enableButtons();
            log('Device connected successfully', 'success');
            await refreshDeviceInfo();
        } else {
            log(`Connection failed: ${result.message}`, 'error');
        }
    } catch (error) {
        log(`Connection error: ${error}`, 'error');
        console.error('Connect error:', error);
    } finally {
        btn.disabled = false;
        btn.textContent = isConnected ? 'Reconnect' : 'Connect Ledger';
    }
}

// Refresh device information
async function refreshDeviceInfo() {
    const btn = document.getElementById('refresh-info-btn');
    const originalText = btn.textContent;
    btn.disabled = true;
    btn.innerHTML = '<span class="loading"></span>Loading...';

    try {
        log('Fetching device info...', 'info');
        const info = await wasm.get_device_info();

        displayDeviceInfo(info);
        updateAppStatus(info);
        log('Device info updated', 'success');
    } catch (error) {
        log(`Failed to get device info: ${error}`, 'error');
        console.error('Get info error:', error);
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
    }
}

// Display device information
function displayDeviceInfo(info) {
    const container = document.getElementById('device-info');

    if (!info.connected) {
        container.innerHTML = '<p class="placeholder">Device not connected</p>';
        return;
    }

    let html = '<dl>';
    html += `<dt>Model</dt><dd>${info.model || 'Unknown'}</dd>`;
    html += `<dt>Firmware</dt><dd>${info.version || 'Unknown'}</dd>`;
    if (info.mcu_version) {
        html += `<dt>MCU</dt><dd>${info.mcu_version}</dd>`;
    }
    html += '</dl>';
    container.innerHTML = html;
}

// Update app status displays
function updateAppStatus(info) {
    // Bitcoin mainnet
    const bitcoinStatus = document.getElementById('bitcoin-status');
    const installBitcoinBtn = document.getElementById('install-bitcoin-btn');
    const updateBitcoinBtn = document.getElementById('update-bitcoin-btn');

    if (info.bitcoin_installed) {
        bitcoinStatus.innerHTML = `
            <p class="app-status installed">
                <strong>Installed</strong> - Version ${info.bitcoin_version || 'Unknown'}
            </p>
        `;
        // Show update button, hide install button when app is installed
        installBitcoinBtn.style.display = 'none';
        updateBitcoinBtn.style.display = '';
        updateBitcoinBtn.disabled = false;
    } else {
        bitcoinStatus.innerHTML = '<p class="app-status not-installed">Not installed</p>';
        // Show install button, hide update button when app is not installed
        installBitcoinBtn.style.display = '';
        installBitcoinBtn.disabled = false;
        updateBitcoinBtn.style.display = 'none';
    }

    // Bitcoin testnet
    const bitcoinTestStatus = document.getElementById('bitcoin-test-status');
    const installBitcoinTestBtn = document.getElementById('install-bitcoin-test-btn');
    const updateBitcoinTestBtn = document.getElementById('update-bitcoin-test-btn');

    if (info.bitcoin_test_installed) {
        bitcoinTestStatus.innerHTML = `
            <p class="app-status installed">
                <strong>Installed</strong> - Version ${info.bitcoin_test_version || 'Unknown'}
            </p>
        `;
        // Show update button, hide install button when app is installed
        installBitcoinTestBtn.style.display = 'none';
        updateBitcoinTestBtn.style.display = '';
        updateBitcoinTestBtn.disabled = false;
    } else {
        bitcoinTestStatus.innerHTML = '<p class="app-status not-installed">Not installed</p>';
        // Show install button, hide update button when app is not installed
        installBitcoinTestBtn.style.display = '';
        installBitcoinTestBtn.disabled = false;
        updateBitcoinTestBtn.style.display = 'none';
    }
}

// Run genuine check
async function runGenuineCheck() {
    const btn = document.getElementById('genuine-btn');
    const resultDiv = document.getElementById('genuine-result');
    const originalText = btn.textContent;
    btn.disabled = true;
    btn.innerHTML = '<span class="loading"></span>Checking...';

    resultDiv.innerHTML = '<div class="result pending">Running genuine check...</div>';

    try {
        log('Running genuine check...', 'info');
        const result = await wasm.genuine_check();

        if (result.success) {
            resultDiv.innerHTML = '<div class="result success">Device is genuine!</div>';
            log('Genuine check passed', 'success');
        } else {
            resultDiv.innerHTML = `<div class="result error">${result.message}</div>`;
            log(`Genuine check: ${result.message}`, 'error');
        }
    } catch (error) {
        resultDiv.innerHTML = `<div class="result error">Error: ${error}</div>`;
        log(`Genuine check error: ${error}`, 'error');
        console.error('Genuine check error:', error);
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
    }
}

// Install Bitcoin app
async function installBitcoinApp(testnet) {
    const btnId = testnet ? 'install-bitcoin-test-btn' : 'install-bitcoin-btn';
    const btn = document.getElementById(btnId);
    const appName = testnet ? 'Bitcoin Test' : 'Bitcoin';
    const originalText = btn.textContent;
    btn.disabled = true;
    btn.innerHTML = '<span class="loading"></span>Installing...';

    try {
        log(`Installing ${appName} app...`, 'info');
        const result = await wasm.install_bitcoin_app(testnet);

        if (result.success) {
            log(`${appName} app installed successfully`, 'success');
            await refreshDeviceInfo();
        } else {
            log(`Install failed: ${result.message}`, 'error');
        }
    } catch (error) {
        log(`Install error: ${error}`, 'error');
        console.error('Install error:', error);
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
    }
}

// Update Bitcoin app
async function updateBitcoinApp(testnet) {
    const btnId = testnet ? 'update-bitcoin-test-btn' : 'update-bitcoin-btn';
    const btn = document.getElementById(btnId);
    const appName = testnet ? 'Bitcoin Test' : 'Bitcoin';
    const originalText = btn.textContent;
    btn.disabled = true;
    btn.innerHTML = '<span class="loading"></span>Updating...';

    try {
        log(`Updating ${appName} app...`, 'info');
        const result = await wasm.update_bitcoin_app(testnet);

        if (result.success) {
            log(`${appName}: ${result.message}`, 'success');
            await refreshDeviceInfo();
        } else {
            log(`Update failed: ${result.message}`, 'error');
        }
    } catch (error) {
        log(`Update error: ${error}`, 'error');
        console.error('Update error:', error);
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
    }
}

// Open Bitcoin app
async function openBitcoinApp(testnet) {
    const btnId = testnet ? 'open-bitcoin-test-btn' : 'open-bitcoin-btn';
    const btn = document.getElementById(btnId);
    const appName = testnet ? 'Bitcoin Test' : 'Bitcoin';
    const originalText = btn.textContent;
    btn.disabled = true;
    btn.innerHTML = '<span class="loading"></span>Opening...';

    try {
        log(`Opening ${appName} app...`, 'info');
        const result = await wasm.open_bitcoin_app(testnet);

        if (result.success) {
            log(`${appName} app opened`, 'success');
        } else {
            log(`Open failed: ${result.message}`, 'error');
        }
    } catch (error) {
        log(`Open error: ${error}`, 'error');
        console.error('Open error:', error);
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
    }
}

// Update connection status display
function updateConnectionStatus(connected) {
    const status = document.getElementById('connection-status');
    if (connected) {
        status.classList.remove('disconnected');
        status.classList.add('connected');
        status.innerHTML = '<span class="status-dot"></span><span>Connected</span>';
    } else {
        status.classList.remove('connected');
        status.classList.add('disconnected');
        status.innerHTML = '<span class="status-dot"></span><span>Not Connected</span>';
    }
}

// Enable action buttons after connection
function enableButtons() {
    document.getElementById('refresh-info-btn').disabled = false;
    document.getElementById('genuine-btn').disabled = false;
    document.getElementById('install-bitcoin-btn').disabled = false;
    document.getElementById('update-bitcoin-btn').disabled = false;
    document.getElementById('open-bitcoin-btn').disabled = false;
    document.getElementById('install-bitcoin-test-btn').disabled = false;
    document.getElementById('update-bitcoin-test-btn').disabled = false;
    document.getElementById('open-bitcoin-test-btn').disabled = false;
}

// Disable all buttons (for unsupported browsers)
function disableAllButtons() {
    document.getElementById('connect-btn').disabled = true;
    document.getElementById('refresh-info-btn').disabled = true;
    document.getElementById('genuine-btn').disabled = true;
    document.getElementById('install-bitcoin-btn').disabled = true;
    document.getElementById('update-bitcoin-btn').disabled = true;
    document.getElementById('open-bitcoin-btn').disabled = true;
    document.getElementById('install-bitcoin-test-btn').disabled = true;
    document.getElementById('update-bitcoin-test-btn').disabled = true;
    document.getElementById('open-bitcoin-test-btn').disabled = true;
}

// Log a message to the activity log
function log(message, type = 'info') {
    const logDiv = document.getElementById('log');
    const time = new Date().toLocaleTimeString();
    const entry = document.createElement('div');
    entry.className = `log-entry ${type}`;
    entry.innerHTML = `<span class="time">${time}</span>${message}`;
    logDiv.appendChild(entry);
    logDiv.scrollTop = logDiv.scrollHeight;
}

// Clear the log
function clearLog() {
    document.getElementById('log').innerHTML = '';
    log('Log cleared', 'info');
}

// Initialize on page load
document.addEventListener('DOMContentLoaded', init);
