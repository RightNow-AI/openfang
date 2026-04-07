"""
End-to-end test for Vertex AI driver.
Tests that the Vertex AI provider works with service account authentication.
"""
import json
import sys
import os

# Service account path
SA_PATH = r"C:\Users\at384\Downloads\osc\dbg-grcit-dev-e1-c79e5571a5a7.json"

def test_vertex_ai():
    try:
        from google.oauth2 import service_account
        from google.auth.transport.requests import Request
    except ImportError:
        print("Installing google-auth...")
        import subprocess
        subprocess.run([sys.executable, "-m", "pip", "install", "google-auth", "-q"])
        from google.oauth2 import service_account
        from google.auth.transport.requests import Request
    
    import urllib.request
    import ssl
    
    # Read project ID from service account
    with open(SA_PATH) as f:
        sa = json.load(f)
    project_id = sa.get("project_id")
    print(f"Project ID: {project_id}")
    print(f"Service Account: {sa.get('client_email')}")
    
    # Get OAuth token using service account
    print("\n=== Getting OAuth Token ===")
    credentials = service_account.Credentials.from_service_account_file(
        SA_PATH,
        scopes=["https://www.googleapis.com/auth/cloud-platform"]
    )
    credentials.refresh(Request())
    token = credentials.token
    print(f"✅ Token obtained: {token[:50]}...")
    
    # Test Vertex AI API
    print("\n=== Testing Vertex AI API ===")
    url = f"https://us-central1-aiplatform.googleapis.com/v1/projects/{project_id}/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent"
    
    payload = {
        "contents": [{
            "role": "user",
            "parts": [{"text": "Say 'Hello from Vertex AI!' exactly, nothing else."}]
        }],
        "generationConfig": {
            "maxOutputTokens": 50
        }
    }
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers=headers,
        method="POST"
    )
    
    try:
        ctx = ssl.create_default_context()
        with urllib.request.urlopen(req, context=ctx, timeout=30) as resp:
            response = json.loads(resp.read().decode())
            text = response["candidates"][0]["content"]["parts"][0]["text"]
            print(f"✅ Vertex AI Response: {text}")
            
            # Check usage
            if "usageMetadata" in response:
                usage = response["usageMetadata"]
                print(f"   Input tokens: {usage.get('promptTokenCount', 'N/A')}")
                print(f"   Output tokens: {usage.get('candidatesTokenCount', 'N/A')}")
            
            return True
    except urllib.error.HTTPError as e:
        print(f"❌ HTTP Error {e.code}: {e.reason}")
        print(f"   Response: {e.read().decode()}")
        return False
    except Exception as e:
        print(f"❌ API call failed: {e}")
        return False

def test_streaming():
    """Test streaming endpoint."""
    try:
        from google.oauth2 import service_account
        from google.auth.transport.requests import Request
    except ImportError:
        return False
    
    import urllib.request
    import ssl
    
    with open(SA_PATH) as f:
        sa = json.load(f)
    project_id = sa.get("project_id")
    
    credentials = service_account.Credentials.from_service_account_file(
        SA_PATH,
        scopes=["https://www.googleapis.com/auth/cloud-platform"]
    )
    credentials.refresh(Request())
    token = credentials.token
    
    print("\n=== Testing Streaming API ===")
    url = f"https://us-central1-aiplatform.googleapis.com/v1/projects/{project_id}/locations/us-central1/publishers/google/models/gemini-2.0-flash:streamGenerateContent?alt=sse"
    
    payload = {
        "contents": [{
            "role": "user",
            "parts": [{"text": "Count from 1 to 5, one number per line."}]
        }],
        "generationConfig": {
            "maxOutputTokens": 100
        }
    }
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers=headers,
        method="POST"
    )
    
    try:
        ctx = ssl.create_default_context()
        with urllib.request.urlopen(req, context=ctx, timeout=30) as resp:
            print("✅ Streaming response:")
            full_text = ""
            for line in resp:
                line = line.decode().strip()
                if line.startswith("data: "):
                    data = json.loads(line[6:])
                    if "candidates" in data:
                        for candidate in data["candidates"]:
                            if "content" in candidate:
                                for part in candidate["content"].get("parts", []):
                                    if "text" in part:
                                        full_text += part["text"]
                                        print(f"   chunk: {part['text']!r}")
            print(f"   Full text: {full_text}")
            return True
    except Exception as e:
        print(f"❌ Streaming failed: {e}")
        return False

if __name__ == "__main__":
    print("="*60)
    print("VERTEX AI END-TO-END TEST")
    print("="*60)
    
    success1 = test_vertex_ai()
    success2 = test_streaming()
    
    print("\n" + "="*60)
    if success1 and success2:
        print("✅ ALL TESTS PASSED")
    else:
        print("❌ SOME TESTS FAILED")
    print("="*60)
    
    sys.exit(0 if (success1 and success2) else 1)
