// Runs before first paint so there is no flash: dark by default, the visitor's saved choice otherwise.
try { var t = localStorage.getItem('sc-theme'); if (t === 'light' || t === 'dark') document.documentElement.dataset.theme = t; } catch (e) {}
