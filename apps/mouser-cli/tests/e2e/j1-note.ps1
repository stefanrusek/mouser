# J1 note window on computer B: one maximised multiline text box titled
# "mouser-j1-note". Ctrl+S writes the exact text to the note file; nothing else
# ever writes it, so the file proves Command-S on the Mac arrived as Ctrl-S.
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Application]::EnableVisualStyles()
$note = 'C:\Users\Public\mouser-j1-note.txt'
$form = New-Object System.Windows.Forms.Form
$form.Text = 'mouser-j1-note'
$form.WindowState = [System.Windows.Forms.FormWindowState]::Maximized
$box = New-Object System.Windows.Forms.TextBox
$box.Multiline = $true
$box.AcceptsReturn = $true
$box.Dock = [System.Windows.Forms.DockStyle]::Fill
$form.Controls.Add($box)
$form.Add_Shown({
    $form.Activate()
    $box.Focus()
    Set-Content -LiteralPath 'C:\Users\Public\mouser-j1-note.hwnd' -Value $form.Handle.ToInt64()
    Set-Content -LiteralPath 'C:\Users\Public\mouser-j1-note.pid' -Value $PID
})
$box.Add_KeyDown({
    param($sender, $e)
    if ($e.Control -and $e.KeyCode -eq [System.Windows.Forms.Keys]::S) {
        $e.Handled = $true
        $e.SuppressKeyPress = $true
        [IO.File]::WriteAllText($note, $box.Text, (New-Object System.Text.UTF8Encoding($false)))
    }
})
[System.Windows.Forms.Application]::Run($form)
